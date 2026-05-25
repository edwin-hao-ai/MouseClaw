//! v0.4.0 · 大模型按需下载 + 镜像 fallback + 进度 emit + 完整性校验
//!
//! 取代 transcribe_stream / punctuation 自带的 curl-子进程粗糙下载。
//! v0.3.x 起 DMG 不再打包模型 (~260MB → 35MB)，首次启动后台下载。
//!
//! ## 设计
//! - `ModelSpec` 描述一组要下载到本地 dir 的 file：每文件有多个 mirror URL +
//!   expected_bytes（用来识别坏文件 / 续传判断 / 进度归一化）。
//! - 单文件流程：
//!   1. 已存在 + bytes 对 → skip ✅
//!   2. `.part` 存在 → 用 `curl -C -` 续传
//!   3. 不存在 → 从 mirrors[0] 起拉；4xx / curl 非零 / 下完字节不对 → 切下一个 mirror
//!   4. 三轮镜像全失败 → `Err`（UI 显示具体哪个 URL 失败了）
//! - 全组下完后跑 ONNX load 试一次（任何模型坏了在这里抓到，删 `.part` 重下）。
//! - 进度：poller 每 300ms 算 `(total_done_bytes / total_expected_bytes)`，
//!   通过 `Emitter::emit("model-progress", ProgressEvent)` 推给前端。
//!
//! ## 镜像（实测 2026-05-19 全部 200 + Content-Length 准确）
//!
//! HF 单文件源（zh-en + 英文模型）：
//!   1. `https://hf-mirror.com` —— 国内无墙（清华系），无 VPN 可达
//!   2. `https://huggingface.co` —— 境外原始
//!
//! GitHub release tarball 源（标点模型）：
//!   1. `https://gh-proxy.com` —— 国内代理
//!   2. `https://ghfast.top/https://github.com` —— 国内代理
//!   3. `https://github.com` —— 境外原始

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use anyhow::{anyhow, Context, Result};
use once_cell::sync::Lazy;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// v0.4.0 · 同步可读的最新进度快照，pipeline.rs / blocked 气泡用来组装消息。
/// key = spec.id（"sherpa-zh-en" / "sherpa-en" / "sherpa-punct"），value = 最新进度。
static LATEST_PROGRESS: Lazy<Mutex<std::collections::HashMap<String, ProgressEvent>>> =
    Lazy::new(|| Mutex::new(std::collections::HashMap::new()));

/// v0.4.0 · 防并发下载 —— 同一 spec.id 同一时刻只允许一个 download 任务在跑。
/// 之前 lib.rs setup + onboarding 完成 + retry 按钮三处都 spawn，
/// 两个 task 同时操作同一个 .part 会互相覆写 → 数据损坏 / 进度乱跳。
use std::sync::atomic::{AtomicBool, Ordering};
use std::collections::HashMap;
static IN_FLIGHT: Lazy<Mutex<HashMap<String, std::sync::Arc<AtomicBool>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn try_lock_download(id: &str) -> Option<std::sync::Arc<AtomicBool>> {
    let mut map = IN_FLIGHT.lock().unwrap();
    let flag = map.entry(id.to_string())
        .or_insert_with(|| std::sync::Arc::new(AtomicBool::new(false)))
        .clone();
    if flag.swap(true, Ordering::SeqCst) {
        return None; // already in flight
    }
    Some(flag)
}

/// RAII guard —— Drop 时自动释放 flag，函数任何 early return / panic 都安全。
struct DownloadGuard(std::sync::Arc<AtomicBool>);
impl Drop for DownloadGuard {
    fn drop(&mut self) { self.0.store(false, Ordering::SeqCst); }
}

pub fn latest_progress(model_id: &str) -> Option<ProgressEvent> {
    LATEST_PROGRESS.lock().unwrap().get(model_id).cloned()
}

/// DownloaderView 挂载时主动拉一次 —— 没监听到 emit 也能恢复状态显示。
/// 对每个已存在但没 progress 记录的 ModelSpec，构造一个虚拟 "ok" event。
pub fn snapshot_all() -> Vec<ProgressEvent> {
    let mut out: Vec<ProgressEvent> = Vec::new();
    let map = LATEST_PROGRESS.lock().unwrap();
    // v0.7 · 语音只剩 SenseVoice（流式 zipformer + 标点模型已弃用）。
    let specs = [sense_voice_spec()];
    for spec in specs {
        if let Some(p) = map.get(spec.id) {
            out.push(p.clone());
        } else if spec.is_ready() {
            // 模型已在本地（首启 ready / 老用户升级）—— 构造一条 "ok" 兜底
            let total = total_bytes(&spec);
            out.push(ProgressEvent {
                model_id: spec.id.to_string(),
                display: spec.display.to_string(),
                file_index: spec.files.len(),
                file_total: spec.files.len(),
                current_file: String::new(),
                current_bytes: 0,
                current_total: 0,
                total_done: total,
                total_expected: total,
                mirror: String::new(),
                phase: "ok",
                message: Some("已就绪".into()),
            });
        }
    }
    out
}

// ────────────────── ModelSpec ──────────────────

#[derive(Debug, Clone)]
pub struct FileSpec {
    /// 落到 `<models_dir>/<spec.id>/<rel_path>` 的相对路径
    pub rel_path: &'static str,
    /// 预期字节数（HEAD 实测结果，写死 = 防 HF reupload 不知不觉换文件）
    pub bytes: u64,
    /// 完整 URL 镜像列表，按优先级排（国内 first）
    pub mirrors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ModelSpec {
    /// 子目录名 + 标识，如 "sherpa-zh-en" / "sherpa-en" / "sherpa-punct"
    pub id: &'static str,
    /// UI 显示名，如 "中文语音模型"
    pub display: &'static str,
    pub files: Vec<FileSpec>,
    /// 若为 Some((filename, bytes))：下载下来的是 tar.bz2，解压后取这一个文件留下，
    /// 其余删掉。标点模型用。filename = 解压后目标文件名；bytes = 该文件预期字节数
    /// （解压完后校验，避免半文件场景 is_ready 误报 true）
    pub post_extract_keep: Option<(&'static str, u64)>,
}

impl ModelSpec {
    /// `<HOME>/.mouseclaw/models/<spec.id>/`
    pub fn dest_dir(&self) -> Result<PathBuf> {
        let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("HOME not set"))?;
        Ok(PathBuf::from(home).join(".mouseclaw/models").join(self.id))
    }

    /// 已全部就绪（所有 file 存在 + 字节数对得上）
    pub fn is_ready(&self) -> bool {
        let dir = match self.dest_dir() { Ok(d) => d, Err(_) => return false };
        // tarball 模型：检查 post_extract_keep 文件 + 字节数（避免半文件场景）
        if let Some((keep, bytes)) = self.post_extract_keep {
            return matches!(std::fs::metadata(dir.join(keep)),
                Ok(m) if m.len() == bytes);
        }
        for f in &self.files {
            let p = dir.join(f.rel_path);
            match std::fs::metadata(&p) {
                Ok(m) if m.len() == f.bytes => {}
                _ => return false,
            }
        }
        true
    }
}

// ────────────────── Progress event ──────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ProgressEvent {
    pub model_id: String,
    pub display: String,
    /// 当前下载到第几个文件（0-indexed），方便 UI 显示「2/4」
    pub file_index: usize,
    pub file_total: usize,
    /// 当前文件相对路径
    pub current_file: String,
    /// 当前文件已下载 bytes
    pub current_bytes: u64,
    /// 当前文件总 bytes
    pub current_total: u64,
    /// 整个模型已下载 bytes
    pub total_done: u64,
    /// 整个模型总 bytes
    pub total_expected: u64,
    /// 当前正使用的镜像 URL（用户能看到 fallback 切换）
    pub mirror: String,
    /// 状态消息：downloading / verifying / fallback / ok / error
    pub phase: &'static str,
    pub message: Option<String>,
}

// ────────────────── Public API ──────────────────

/// 异步下载一个模型，进度通过 `app.emit("model-progress", ProgressEvent)` 推送。
/// 完成 / 失败时 emit `phase=ok` / `phase=error`。
pub async fn download(app: AppHandle, spec: ModelSpec) -> Result<()> {
    // v0.4.0 · 防并发：同 spec 已有 task 在跑 → 直接 emit 当前快照（如有），不再开新任务
    let _guard = match try_lock_download(spec.id) {
        Some(flag) => DownloadGuard(flag),
        None => {
            println!("[mouseclaw] 🔒 {} 已有下载任务在跑，跳过", spec.id);
            // 让调用方 emit 一次 latest（DownloaderView 才知道有人在下）
            if let Some(p) = latest_progress(spec.id) {
                let _ = app.emit("model-progress", p);
            }
            return Ok(());
        }
    };

    let dir = spec.dest_dir()?;
    tokio::fs::create_dir_all(&dir).await
        .with_context(|| format!("mkdir {}", dir.display()))?;

    if spec.is_ready() {
        let total = total_bytes(&spec);
        emit_progress(&app, &spec, spec.files.len(), "ok",
            0, 0, total, total,
            "", Some("已就绪".into()));
        return Ok(());
    }

    let total_expected: u64 = total_bytes(&spec);

    // 主下载循环：每个文件 → 每个镜像 → curl 续传
    for (idx, file) in spec.files.iter().enumerate() {
        let dest = dir.join(file.rel_path);
        let part = dest.with_extension(format!(
            "{}.part",
            dest.extension().and_then(|s| s.to_str()).unwrap_or("dl")
        ));

        // 跳过已就绪的文件
        if let Ok(m) = std::fs::metadata(&dest) {
            if m.len() == file.bytes {
                println!("[mouseclaw] ✅ {} 已就绪 ({} bytes)", file.rel_path, file.bytes);
                continue;
            }
        }

        // 创建父目录（tarball 路径含 / 时需要）
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await.ok();
        }

        let mut ok = false;
        let mut last_err = String::new();

        for (mi, mirror) in file.mirrors.iter().enumerate() {
            emit_progress(&app, &spec, idx, "downloading",
                fsize(&part), file.bytes,
                done_so_far(&spec, &dir, idx) + fsize(&part), total_expected,
                mirror, Some(format!("镜像 {}/{}: {}", mi + 1, file.mirrors.len(), short_host(mirror))));

            match download_one(&app, &spec, idx, mirror, &part, file.bytes, total_expected, &dir).await {
                Ok(()) => {
                    // 字节校验
                    let actual = fsize(&part);
                    if actual != file.bytes {
                        last_err = format!(
                            "字节数对不上: 实际 {} ≠ 预期 {}（镜像 {} 可能给了坏数据）",
                            actual, file.bytes, short_host(mirror)
                        );
                        let _ = tokio::fs::remove_file(&part).await;
                        emit_progress(&app, &spec, idx, "fallback",
                            0, file.bytes, done_so_far(&spec, &dir, idx), total_expected,
                            mirror, Some(last_err.clone()));
                        continue;
                    }
                    // rename .part → final
                    if let Err(e) = tokio::fs::rename(&part, &dest).await {
                        last_err = format!("rename {} 失败: {}", file.rel_path, e);
                        continue;
                    }
                    ok = true;
                    break;
                }
                Err(e) => {
                    last_err = format!("{} 失败: {}", short_host(mirror), e);
                    emit_progress(&app, &spec, idx, "fallback",
                        fsize(&part), file.bytes,
                        done_so_far(&spec, &dir, idx) + fsize(&part), total_expected,
                        mirror, Some(last_err.clone()));
                    // v0.4.0 · 关键 bug 修复：切镜像前必须删 .part。
                    // 之前是「不删，让下个镜像续传」—— 但 curl `-C -` + 不同镜像
                    // 不保证 Range 一致，会把数据 append 到 .part 上，造成字节翻倍
                    // （实测 212MB/189MB 111.9%）。同镜像内部 curl `--retry 2`
                    // 已经 handle 短期失败，跨镜像就重头来。
                    let _ = tokio::fs::remove_file(&part).await;
                }
            }
        }

        if !ok {
            let msg = format!("文件 {} 所有镜像都失败 · 最后错误: {}", file.rel_path, last_err);
            emit_progress(&app, &spec, idx, "error",
                fsize(&part), file.bytes,
                done_so_far(&spec, &dir, idx), total_expected,
                "", Some(msg.clone()));
            return Err(anyhow!(msg));
        }
    }

    // tarball 解压（标点模型）+ 字节校验
    if let Some((keep, keep_bytes)) = spec.post_extract_keep {
        emit_progress(&app, &spec, spec.files.len(), "verifying",
            0, 0, total_expected, total_expected,
            "", Some("解压 tar.bz2…".into()));
        if let Err(e) = extract_tarball(&dir, spec.files[0].rel_path, keep).await {
            emit_progress(&app, &spec, spec.files.len(), "error",
                0, 0, total_expected, total_expected,
                "", Some(format!("解压失败: {e}")));
            return Err(e);
        }
        // 校验解压后字节数
        let extracted_path = dir.join(keep);
        let actual = fsize(&extracted_path);
        if actual != keep_bytes {
            let _ = tokio::fs::remove_file(&extracted_path).await;
            let msg = format!("解压后字节不对: 实际 {} ≠ 预期 {}", actual, keep_bytes);
            emit_progress(&app, &spec, spec.files.len(), "error",
                0, 0, total_expected, total_expected,
                "", Some(msg.clone()));
            return Err(anyhow!(msg));
        }
    }

    emit_progress(&app, &spec, spec.files.len(), "ok",
        0, 0, total_expected, total_expected,
        "", Some("下载完成".into()));
    println!("[mouseclaw] 🎤 model {} 下载完成 → {}", spec.id, dir.display());
    Ok(())
}

// ────────────────── 单文件 curl 续传 ──────────────────

async fn download_one(
    app: &AppHandle,
    spec: &ModelSpec,
    idx: usize,
    url: &str,
    part: &Path,
    expected_bytes: u64,
    total_expected: u64,
    dir: &Path,
) -> Result<()> {
    // v0.4.0 · 防御：.part 字节数若已 ≥ expected（之前 append 写坏了 / 上次完整下完
    // 但 rename 失败），直接清掉重头来。否则 curl `-C -` 看到 part 完整会 304 退出，
    // 我们随后字节校验也会过（但内容可能是坏的）。多花点时间换正确性。
    if let Ok(meta) = std::fs::metadata(part) {
        if meta.len() >= expected_bytes {
            let _ = tokio::fs::remove_file(part).await;
        }
    }

    // 启动 curl 子进程：
    //   -fL          : 失败时非零退出 + 跟随重定向
    //   -C -         : 自动从 .part 已有字节续传
    //   --retry 2    : 失败重试两次
    //   --retry-delay 2
    //   -A           : UA（部分 mirror 看 UA）
    //   -s           : 静默（我们自己读文件大小推进度）
    //   -o <part>    : 输出到临时文件
    //   --connect-timeout 10
    //   --max-time   : 不设，大文件慢网络场景不能预设
    let mut cmd = tokio::process::Command::new("curl");
    cmd.args([
        "-fL", "-C", "-",
        // v0.4.0 · retry 更激进 —— exit 18 (partial transfer) / 5xx / 抖动都重试。
        // 从 2 次 → 5 次，间隔 5 秒，给 hf-mirror 临时抖动恢复时间。
        // --retry-all-errors 让 curl 把所有 4xx/5xx 都纳入重试（默认只 transient）
        "--retry", "5", "--retry-delay", "5", "--retry-all-errors",
        "-A", "MouseClaw/0.4 (https://github.com/edwin-hao-ai/MouseClaw)",
        "-s",
        "--connect-timeout", "10",
        // stall detection：连上后 30 秒速度 <1KB/s 直接断 → 主循环切下一个 mirror
        "--speed-time", "30",
        "--speed-limit", "1024",
        "-o",
    ]);
    cmd.arg(part);
    cmd.arg(url);
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().context("spawn curl")?;
    let part_owned = part.to_path_buf();

    // Progress poller：每 300ms 看 .part 大小，emit 给前端
    let app_clone = app.clone();
    let spec_clone = spec.clone();
    let url_owned = url.to_string();
    let dir_owned = dir.to_path_buf();
    let poller = tokio::spawn(async move {
        let started = Instant::now();
        loop {
            tokio::time::sleep(Duration::from_millis(300)).await;
            let cur = fsize(&part_owned);
            let total_done = done_so_far(&spec_clone, &dir_owned, idx) + cur;
            emit_progress(&app_clone, &spec_clone, idx, "downloading",
                cur, expected_bytes, total_done, total_expected,
                &url_owned, None);
            if started.elapsed() > Duration::from_secs(60 * 30) {
                break;
            }
        }
    });

    // v0.4.0 · RAII —— `child.wait()?` 抛错时也要中止 poller，否则它继续 emit
    // 假的 downloading 进度（实测会导致 UI 在 error 状态下还在跳数字）
    struct PollerGuard(tokio::task::JoinHandle<()>);
    impl Drop for PollerGuard {
        fn drop(&mut self) { self.0.abort(); }
    }
    let _poller_guard = PollerGuard(poller);

    let status = child.wait().await.context("curl wait")?;

    if !status.success() {
        // 拿 stderr 给个错误
        let mut err = String::new();
        if let Some(mut s) = child.stderr.take() {
            use tokio::io::AsyncReadExt;
            let _ = s.read_to_string(&mut err).await;
        }
        let code = status.code().unwrap_or(-1);
        let label = curl_exit_label(code);
        return Err(anyhow!("curl exit {code} ({label}): {}", err.trim()));
    }
    Ok(())
}

/// v0.4.0 · 把 curl exit code 翻译成人能读的短描述。
/// 用户看到「Some(18)」会困惑；看到「下载被中断（partial transfer）」就懂了。
fn curl_exit_label(code: i32) -> &'static str {
    match code {
        6  => "DNS 解析失败 - 镜像域名解析不到",
        7  => "连不上服务器",
        18 => "下载被中断（partial transfer · 远端断了连接）",
        22 => "HTTP 错误（404 / 403 / 5xx）",
        28 => "超时（30 秒内速度低于 1KB/s）",
        35 => "TLS/SSL 握手失败",
        52 => "服务器返回空响应",
        56 => "接收数据时失败（连接被远端断）",
        92 => "HTTP/2 流错误",
        _  => "未知错误",
    }
}

// ────────────────── tarball 解压（标点模型） ──────────────────

async fn extract_tarball(dir: &Path, tar_rel: &str, keep_name: &str) -> Result<()> {
    let tar_path = dir.join(tar_rel);
    let dest = dir.join(keep_name);

    // 用 macOS 自带 tar，支持 -j (bzip2)。整个解压 IO 拉到 spawn_blocking
    // 避免占用 tokio runtime worker thread（62MB tar 解压 ~1-2 秒）
    let status = tokio::process::Command::new("tar")
        .arg("-xjf")
        .arg(&tar_path)
        .arg("-C")
        .arg(dir)
        .status().await.context("spawn tar")?;
    if !status.success() {
        return Err(anyhow!("tar exit {:?}（可能 tarball 损坏，重试会清掉重下）", status.code()));
    }

    let dir_owned = dir.to_path_buf();
    let dest_owned = dest.clone();
    let tar_path_owned = tar_path.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let found = find_file_in_subdirs(&dir_owned, "model.int8.onnx")?;
        if let Some(src) = found {
            if src != dest_owned {
                std::fs::rename(&src, &dest_owned)
                    .with_context(|| format!("mv {} → {}", src.display(), dest_owned.display()))?;
            }
        } else {
            return Err(anyhow!("解压后未找到 model.int8.onnx"));
        }
        // 清掉 tarball + 多余目录（保留 keep_name 这个 final 文件）
        let _ = std::fs::remove_file(&tar_path_owned);
        if let Ok(rd) = std::fs::read_dir(&dir_owned) {
            for entry in rd.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let _ = std::fs::remove_dir_all(&p);
                }
            }
        }
        Ok(())
    }).await.context("spawn_blocking extract cleanup")??;
    Ok(())
}

fn find_file_in_subdirs(dir: &Path, name: &str) -> Result<Option<PathBuf>> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("readdir {}", dir.display()))? {
        let entry = entry?;
        let p = entry.path();
        if p.is_file() && p.file_name().and_then(|s| s.to_str()) == Some(name) {
            return Ok(Some(p));
        }
        if p.is_dir() {
            for sub in std::fs::read_dir(&p)? {
                let sub = sub?;
                let sp = sub.path();
                if sp.is_file() && sp.file_name().and_then(|s| s.to_str()) == Some(name) {
                    return Ok(Some(sp));
                }
            }
        }
    }
    Ok(None)
}

// ────────────────── 辅助函数 ──────────────────

fn fsize(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

fn total_bytes(spec: &ModelSpec) -> u64 {
    spec.files.iter().map(|f| f.bytes).sum()
}

fn done_so_far(spec: &ModelSpec, dir: &Path, current_idx: usize) -> u64 {
    spec.files.iter().take(current_idx)
        .map(|f| fsize(&dir.join(f.rel_path)))
        .sum()
}

fn short_host(url: &str) -> &str {
    url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))
        .and_then(|s| s.split('/').next())
        .unwrap_or(url)
}

fn emit_progress(
    app: &AppHandle,
    spec: &ModelSpec,
    file_index: usize,
    phase: &'static str,
    current_bytes: u64,
    current_total: u64,
    total_done: u64,
    total_expected: u64,
    mirror: &str,
    message: Option<String>,
) {
    // v0.4.0 · clamp 进度数值 —— curl `-C -` 续传 + mirror 切换会让 .part 大小
    // 偶尔大于 expected_bytes（实测 212MB/189MB · 111.9%）。前端看到 >100% 会困惑，
    // 后端兜底 min 一下；真正的字节校验仍在 download() 完成时做，clamp 只影响 UI 显示。
    let cur_b = current_bytes.min(current_total.max(1));
    let tot_d = total_done.min(total_expected.max(1));
    // file_index 显示 cap：解压 / 完成阶段 idx 会传 files.len()，前端 +1 显示就变 N+1/N
    let idx_display = file_index.min(spec.files.len().saturating_sub(1));
    let evt = ProgressEvent {
        model_id: spec.id.to_string(),
        display: spec.display.to_string(),
        file_index: idx_display,
        file_total: spec.files.len(),
        current_file: spec.files.get(idx_display)
            .map(|f| f.rel_path.to_string())
            .unwrap_or_default(),
        current_bytes: cur_b,
        current_total,
        total_done: tot_d,
        total_expected,
        mirror: mirror.to_string(),
        phase,
        message,
    };
    LATEST_PROGRESS.lock().unwrap().insert(spec.id.to_string(), evt.clone());
    let _ = app.emit("model-progress", evt);
}

// ────────────────── 内建 ModelSpec 工厂 ──────────────────
// v0.7 · 旧流式 zh-en zipformer + 英文模型 + CT-Transformer 标点 spec 全删 ——
// 语音统一用 SenseVoice（自带标点）。

/// SenseVoice 多语种离线模型 · int8 ~228MB（model.int8.onnx + tokens.txt）
/// `sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17`
/// 比 2023 流式 zipformer 中英混说质量高一档 + 自带标点（use_itn）。离线（松手转写）。
pub fn sense_voice_spec() -> ModelSpec {
    let hf_repo = "csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17";
    let mirrors_for = |file: &str| vec![
        // hf-mirror 对该 repo 的 LFS 解析有问题（返回 pointer），huggingface 优先。
        format!("https://huggingface.co/{hf_repo}/resolve/main/{file}"),
        format!("https://hf-mirror.com/{hf_repo}/resolve/main/{file}"),
    ];
    ModelSpec {
        id: "sense-voice",
        display: "SenseVoice 多语种语音模型",
        files: vec![
            FileSpec {
                rel_path: "model.int8.onnx",
                bytes: 239_233_841,
                mirrors: mirrors_for("model.int8.onnx"),
            },
            FileSpec {
                rel_path: "tokens.txt",
                bytes: 315_894,
                mirrors: mirrors_for("tokens.txt"),
            },
        ],
        post_extract_keep: None,
    }
}

// v0.7 · punct_spec 已删 —— SenseVoice use_itn 自带标点。
