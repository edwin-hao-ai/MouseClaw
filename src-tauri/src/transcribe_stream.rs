//! Streaming ASR via sherpa-onnx + Zipformer-transducer (v0.2).
//!
//! Replaced Whisper batch transcription (transcribe.rs deleted in v0.3).
//! Designed for real-time "边说边出字" — partials emit every ~200ms while
//! user holds the shortcut, final pulled on release.
//!
//! ### Model
//!   - sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20
//!   - ~180 MB on disk (encoder int8 + decoder + joiner int8 + tokens)
//!   - Auto-downloaded from huggingface on first launch into
//!     `~/.mouseclaw/models/sherpa-zh-en/`
//!   - WER on CommonVoice-zh comparable to Whisper Small (8-12%)
//!
//! ### Resource discipline
//!   - `OnlineRecognizer` cached behind once_cell::Lazy + Mutex
//!   - Loaded on first transcribe (cold start ~500ms)
//!   - Streams (`OnlineStream`) created per-utterance, dropped after finalize
//!
//! ### Privacy
//!   - All inference on-device via onnxruntime CPU/CoreML
//!   - No network calls at runtime (model download only happens once, opt-in)

use std::path::PathBuf;
use std::sync::Mutex;
use anyhow::{anyhow, Context, Result};
use once_cell::sync::Lazy;
use sherpa_onnx::{OnlineRecognizer, OnlineRecognizerConfig, OnlineStream};
use tauri::AppHandle;

/// Model relative dir under ~/.mouseclaw/models/ —— **中文模型**
pub const MODEL_DIR: &str = "sherpa-zh-en";

/// v0.6 · BPE vocab（SentencePiece，~12 KB）—— 编译进二进制。
/// 双语 zh-en Zipformer 的英文建模单元是大写 BPE piece（`▁PUSH` / `▁THE`…）。
/// 没它时 sherpa 把英文 hotword（"push"）当整 token 查表 → 永远查不到 →
/// 英文 contextual biasing 形同虚设，"push" 被音译成"铺石"。
/// 设 `modeling_unit=cjkchar+bpe` + 这份 vocab，sherpa 会把英文 hotword 正确
/// 切成模型 units 来 boost。随模型版本锁定（zh-en-2023-02-20），故内嵌而非下载：
/// 离线可用、对老安装即时生效、只占 12 KB。
const BPE_VOCAB: &str = include_str!("../resources/sherpa/bpe.vocab");
/// 落盘文件名（sherpa 需要文件路径，不收 in-memory buffer）。
const BPE_VOCAB_FILE: &str = "bpe.vocab";
/// 中文模型文件名（4 个）—— 用于 ensure_loaded / bundle seed 拼路径
pub const MODEL_FILES: &[&str] = &[
    "encoder-epoch-99-avg-1.int8.onnx",
    "decoder-epoch-99-avg-1.onnx",
    "joiner-epoch-99-avg-1.int8.onnx",
    "tokens.txt",
];

/// v0.4.0 P0：锁死 zh-en 双语模型 —— 中英混说零切换。
/// 旧 config.voice_lang ("zh" / "en") 保留字段做向后兼容但**被忽略**。
/// 原因：sherpa zh-en Zipformer 本身双语联合训练，比单语模型在 code-switch
/// 场景体感强很多；强制让用户二选一是历史包袱，UX 上是个 bug。
pub fn active_lang() -> String { "zh-en".into() }
fn is_en() -> bool { false }
pub fn active_model_dir() -> &'static str { MODEL_DIR }
pub fn active_model_files() -> &'static [&'static str] { MODEL_FILES }
pub fn active_spec() -> crate::model_downloader::ModelSpec {
    crate::model_downloader::zh_en_spec()
}

/// Cached recognizer —— first transcribe loads it, subsequent ones reuse.
static RECOGNIZER: Lazy<Mutex<Option<OnlineRecognizer>>> = Lazy::new(|| Mutex::new(None));

/// Track download state so UI can show progress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelState {
    Ready,
    Downloading,
    Failed(String),
}
static DOWNLOAD_STATE: Lazy<Mutex<Option<ModelState>>> = Lazy::new(|| Mutex::new(None));

pub fn current_state() -> Option<ModelState> {
    DOWNLOAD_STATE.lock().unwrap().clone()
}

fn models_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("HOME not set"))?;
    Ok(PathBuf::from(home).join(".mouseclaw/models").join(active_model_dir()))
}

/// True iff active 语言模型的所有文件齐了（字节校验由 ModelSpec::is_ready 做）
pub fn is_ready() -> bool { active_spec().is_ready() }

/// 启动时调一次 (v0.4.0)：
///   1. active 语言模型已就绪 → Ready，跳过
///   2. App bundle 里仍有打包中文模型（dev / 老 DMG）→ seed 兜底
///   3. 都没有 → 后台 `model_downloader::download(active_spec())`
pub fn kick_off_download_if_missing(app: AppHandle) {
    let spec = active_spec();
    let was_ready = spec.is_ready();
    if was_ready {
        *DOWNLOAD_STATE.lock().unwrap() = Some(ModelState::Ready);
    } else {
        // dev / 老 DMG bundle 兜底（仅中文 —— 英文模型从来没塞过 bundle）
        if !is_en() && try_seed_from_bundle().unwrap_or(false) && spec.is_ready() {
            *DOWNLOAD_STATE.lock().unwrap() = Some(ModelState::Ready);
            println!("[mouseclaw] 🎤 sherpa zh-en 从 bundle seed");
        } else {
            *DOWNLOAD_STATE.lock().unwrap() = Some(ModelState::Downloading);
            println!("[mouseclaw] 🎤 {} 后台下载", spec.display);
        }
    }

    // 模型已在磁盘上 → 立即后台预热 recognizer，别等用户首次按键才同步加载。
    // （首次 OnlineRecognizer::create 要几百 ms ~ 数秒，卡在按键路径里会让第一次
    //  按 fn 唤不出语音 / 与松开竞态丢首次录音 —— 这是 v0.4 首次触发延迟的根因）
    if was_ready {
        preload();
    }

    // 永远走一遍 download() —— ready 时它会立即 emit "ok"（给 DownloaderView），
    // 不 ready 时正常跑下载链路。两路统一减少 bug。
    let display = spec.display.to_string();
    tauri::async_runtime::spawn(async move {
        match crate::model_downloader::download(app, spec).await {
            Ok(()) => {
                *DOWNLOAD_STATE.lock().unwrap() = Some(ModelState::Ready);
                if !was_ready {
                    println!("[mouseclaw] 🎤 {display} 下载完成");
                    // 刚下完 → 预热，让用户紧接着的第一次按键就快
                    preload();
                }
            }
            Err(e) => {
                *DOWNLOAD_STATE.lock().unwrap() =
                    Some(ModelState::Failed(e.to_string()));
                eprintln!("[mouseclaw] 🎤 {display} 下载失败: {e:#}");
            }
        }
    });
}

/// 启动 / 下载完成后调一次：在后台线程把 recognizer 加载进内存，
/// 这样用户第一次按 fn / ⌘⇧Space 时 `StreamSession::new()` 是即时的，
/// 不会同步卡在 `OnlineRecognizer::create`（首次几百 ms ~ 数秒）上。
/// 失败不致命 —— 真正用到时 `ensure_loaded` 会再试一次并 surface 错误。
pub fn preload() {
    std::thread::Builder::new()
        .name("mouseclaw-asr-warmup".into())
        .spawn(|| {
            if !is_ready() { return; }
            // 已加载则 ensure_loaded 立即返回（guard.is_some()）
            match ensure_loaded() {
                Ok(()) => println!("[mouseclaw] 🎤 recognizer warmed up (startup preload)"),
                Err(e) => eprintln!("[mouseclaw] 🎤 recognizer warm-up failed: {e:#}"),
            }
        })
        .ok();
}

/// v0.4.2 · 推理线程数 —— 取机器逻辑核数的一半，clamp 到 2..=4。
/// ASR（流式 Zipformer）与标点（offline CT-Transformer）共用：多核 Apple Silicon
/// 上从 sherpa 默认单线程提到 2~4 是确定收益；对 150ms 小 chunk 过多线程反而增加
/// 调度开销，所以封顶 4。拿不到核数信息（罕见）退回 2。
pub fn pick_inference_threads() -> i32 {
    std::thread::available_parallelism()
        .map(|n| (n.get() / 2).clamp(2, 4) as i32)
        .unwrap_or(2)
}

/// Lazy-load the OnlineRecognizer on first call.
fn ensure_loaded() -> Result<()> {
    let mut guard = RECOGNIZER.lock().unwrap();
    if guard.is_some() { return Ok(()); }

    let dir = models_dir()?;
    if !is_ready() {
        return Err(anyhow!(
            "sherpa model not ready under {} — wait for download",
            dir.display()
        ));
    }

    let files = active_model_files();
    let mut config = OnlineRecognizerConfig::default();

    // v0.4.2 · 性能：sherpa 默认 num_threads=1（单线程 CPU）—— 在多核 Apple Silicon
    // 上是浪费。onnxruntime CPU EP 对 Zipformer encoder 的矩阵乘多线程友好，取一半
    // 核（clamp 2..=4）并行解码，实测能缩短 partial 刷新 + finalize 尾段延迟。
    // provider 仍保持默认 "cpu"：int8 量化模型走 CoreML EP 容易 fallback 反而更慢，
    // 留作后续单独实测的可选项，不在这一刀默认开（先稳后快）。
    let threads = pick_inference_threads();
    config.model_config.num_threads = threads;
    println!("[mouseclaw] 🎤 sherpa ASR num_threads={threads}");

    config.model_config.transducer.encoder = Some(
        dir.join(files[0]).to_string_lossy().into_owned(),
    );
    config.model_config.transducer.decoder = Some(
        dir.join(files[1]).to_string_lossy().into_owned(),
    );
    config.model_config.transducer.joiner = Some(
        dir.join(files[2]).to_string_lossy().into_owned(),
    );
    config.model_config.tokens = Some(
        dir.join(files[3]).to_string_lossy().into_owned(),
    );
    // v0.3.1 · push-to-talk 模式不要 endpoint 自动切段 —— 用户主动控制开始/结束
    // 如果开 endpoint，sherpa 在停顿时会自动 commit 一段并 reset stream，
    // 之后 get_result 只返回新段的文字（旧段被丢掉），用户感觉"流式断了"。
    config.enable_endpoint = false;

    // v0.4.0 P1 · 术语库（hotwords contextual biasing）
    // 1. 重新生成 active.txt（合并内置 + 用户词表）
    // 2. 如果非空，注入 hotwords_file 并切到 modified_beam_search
    //    （greedy_search 不支持 hotwords — sherpa 文档明确说明）
    // 3. 词表空 → 退回 greedy（性能更优一点点）
    let cfg = crate::config::Config::load();
    let active_count = crate::vocab::regenerate_active(cfg.vocab_builtin_enabled)
        .unwrap_or(0);
    let active_path = crate::vocab::active_file_path().ok();
    if active_count > 0 && active_path.as_ref().map(|p| p.exists()).unwrap_or(false) {
        config.decoding_method = Some("modified_beam_search".into());
        config.max_active_paths = 4;
        config.hotwords_file = active_path.map(|p| p.to_string_lossy().into_owned());
        config.hotwords_score = crate::vocab::DEFAULT_HOTWORDS_SCORE;

        // v0.6 · 让英文 hotword 真正生效：把内嵌 bpe.vocab 落盘（缺则写），
        // 设 modeling_unit=cjkchar+bpe + bpe_vocab，sherpa 才会把 "PUSH" 这类英文
        // 词切成模型的大写 BPE units 来 boost。没这步英文 biasing 是 no-op。
        // 失败（磁盘只读等）只降级到"中文 biasing 仍在、英文照旧"，不阻断识别。
        let bpe_path = dir.join(BPE_VOCAB_FILE);
        if !bpe_path.exists() {
            if let Err(e) = std::fs::write(&bpe_path, BPE_VOCAB) {
                eprintln!("[mouseclaw] 🎤 写 bpe.vocab 失败（英文 biasing 降级）: {e}");
            }
        }
        if bpe_path.exists() {
            config.model_config.modeling_unit = Some("cjkchar+bpe".into());
            config.model_config.bpe_vocab = Some(bpe_path.to_string_lossy().into_owned());
        }

        println!(
            "[mouseclaw] 🎤 hotwords loaded: {} entries, score={}, bpe={}",
            active_count, crate::vocab::DEFAULT_HOTWORDS_SCORE, bpe_path.exists()
        );
    } else {
        config.decoding_method = Some("greedy_search".into());
    }

    let rec = OnlineRecognizer::create(&config)
        .ok_or_else(|| anyhow!("OnlineRecognizer::create returned None"))?;
    *guard = Some(rec);
    println!("[mouseclaw] 🎤 sherpa streaming recognizer loaded");
    Ok(())
}

/// v0.4.0 P1 · 强制下次 transcribe 重建 recognizer ——
/// 用于词表/术语库改了之后让 sherpa 重读 hotwords_file。
/// 当前正在录音的 stream 不受影响（trade-off，OK）。
pub fn invalidate_recognizer() {
    let mut guard = RECOGNIZER.lock().unwrap();
    if guard.is_some() {
        println!("[mouseclaw] 🎤 recognizer invalidated (vocab reload)");
        *guard = None;
    }
}

/// One streaming utterance —— created on press, fed via accept(), final on
/// release. Drops the underlying OnlineStream when done.
///
/// Decoding loop pattern (from sherpa-onnx docs):
///   accept_waveform → while is_ready { decode } → get_result
pub struct StreamSession {
    stream: OnlineStream,
}

impl StreamSession {
    pub fn new() -> Result<Self> {
        ensure_loaded()?;
        let stream = {
            let g = RECOGNIZER.lock().unwrap();
            g.as_ref().unwrap().create_stream()
        };
        Ok(Self { stream })
    }

    /// Feed a chunk of 16 kHz mono f32 samples. Triggers internal decode.
    /// Safe to call from a tokio task every 200ms.
    pub fn accept(&mut self, samples: &[f32]) {
        if samples.is_empty() { return; }
        self.stream.accept_waveform(16_000, samples);
        let g = RECOGNIZER.lock().unwrap();
        let rec = g.as_ref().unwrap();
        while rec.is_ready(&self.stream) {
            rec.decode(&self.stream);
        }
    }

    /// Current best-effort partial transcript. Cheap to call.
    pub fn partial(&self) -> String {
        let g = RECOGNIZER.lock().unwrap();
        match g.as_ref().unwrap().get_result(&self.stream) {
            Some(r) => r.text,
            None    => String::new(),
        }
    }

    /// Called once on shortcut release — flush trailing audio + return final.
    pub fn finalize(self) -> Result<String> {
        self.stream.input_finished();
        let g = RECOGNIZER.lock().unwrap();
        let rec = g.as_ref().unwrap();
        while rec.is_ready(&self.stream) {
            rec.decode(&self.stream);
        }
        let text = rec.get_result(&self.stream)
            .map(|r| r.text)
            .unwrap_or_default();
        Ok(text)
    }
}

/// 找 bundle 里的 sherpa-zh-en 资源。Tauri 安装到
/// `MouseClaw.app/Contents/Resources/models/sherpa-zh-en/<file>`。
/// 当前 exe 在 `MouseClaw.app/Contents/MacOS/mouseclaw`，资源在
/// `current_exe()/../../Resources/models/sherpa-zh-en/`。
/// Dev `cargo run` 走 `CARGO_MANIFEST_DIR/resources/sherpa-zh-en/` fallback。
fn bundle_resource_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let bundled = exe.parent()?.parent()?.join("Resources").join("models").join(MODEL_DIR);
    if MODEL_FILES.iter().all(|f| bundled.join(f).exists()) {
        return Some(bundled);
    }
    // dev fallback —— 仅 debug build。release 二进制不走，否则开发机上 release
    // DMG 会读到 CARGO_MANIFEST_DIR/resources/ 里残留的开发期模型，导致下载链路
    // 永远不被触发（用户实测 2026-05-19）
    #[cfg(debug_assertions)]
    {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join(MODEL_DIR);
        if MODEL_FILES.iter().all(|f| dev.join(f).exists()) {
            return Some(dev);
        }
    }
    None
}

/// 第一次启动从 app bundle 拷模型到 ~/.mouseclaw/models/sherpa-zh-en/。
/// 成功返回 true，资源不存在返回 false（dev 模式 / 未打包）。
fn try_seed_from_bundle() -> Result<bool> {
    let Some(src_dir) = bundle_resource_dir() else { return Ok(false); };
    let dest_dir = models_dir()?;
    std::fs::create_dir_all(&dest_dir)?;
    for file in MODEL_FILES {
        let src = src_dir.join(file);
        let dest = dest_dir.join(file);
        if dest.exists() { continue; }
        std::fs::copy(&src, &dest)
            .with_context(|| format!("seed copy {}", file))?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_dir_path_uses_home() {
        if let Ok(dir) = models_dir() {
            assert!(dir.ends_with(MODEL_DIR));
        }
    }

    #[test]
    fn model_files_count_is_four() {
        assert_eq!(MODEL_FILES.len(), 4);
    }

    #[test]
    fn is_ready_returns_bool_doesnt_panic() {
        let _ = is_ready();
    }

    #[test]
    fn current_state_doesnt_panic_before_kickoff() {
        let _ = current_state();
    }

    // ── 真音频 A/B/C 实测（中英混合 "push" 修复）──────────────────────────
    // 依赖本机模型 (~/.mouseclaw/models/sherpa-zh-en) + 一个 16k 单声道 wav，
    // 故默认 #[ignore]，手动跑：
    //   say -v Meijia -o /tmp/mc_push.aiff "我要把代码 push 上去"
    //   afconvert -f WAVE -d LEI16@16000 -c 1 /tmp/mc_push.aiff /tmp/mc_push.wav
    //   cargo test --manifest-path src-tauri/Cargo.toml --lib \
    //     transcribe_stream::tests::ab_mixed_zh_en -- --ignored --nocapture
    // 可用 MC_TEST_WAV 指定其它 wav。打印三种解码结果对比。

    /// 解析 16-bit PCM 单声道 wav → f32 [-1,1]。找 `data` chunk（容忍 FLLR 等填充块）。
    fn read_wav_i16_mono(path: &std::path::Path) -> Vec<f32> {
        let bytes = std::fs::read(path).expect("read wav");
        assert!(&bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE", "not a WAVE file");
        let mut pos = 12usize;
        while pos + 8 <= bytes.len() {
            let id = &bytes[pos..pos + 4];
            let sz = u32::from_le_bytes([bytes[pos+4], bytes[pos+5], bytes[pos+6], bytes[pos+7]]) as usize;
            let body = pos + 8;
            if id == b"data" {
                let end = (body + sz).min(bytes.len());
                return bytes[body..end]
                    .chunks_exact(2)
                    .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
                    .collect();
            }
            pos = body + sz + (sz & 1); // chunk 偶字节对齐
        }
        panic!("no data chunk");
    }

    /// 用给定配置建 recognizer 并转写整段 samples，返回 raw text（模型大写英文）。
    fn transcribe_with(
        dir: &std::path::Path,
        modeling_unit: Option<&str>,
        bpe_vocab: Option<&std::path::Path>,
        hotwords_file: Option<&std::path::Path>,
        samples: &[f32],
    ) -> String {
        let mut config = OnlineRecognizerConfig::default();
        config.model_config.num_threads = 2;
        config.model_config.transducer.encoder =
            Some(dir.join("encoder-epoch-99-avg-1.int8.onnx").to_string_lossy().into_owned());
        config.model_config.transducer.decoder =
            Some(dir.join("decoder-epoch-99-avg-1.onnx").to_string_lossy().into_owned());
        config.model_config.transducer.joiner =
            Some(dir.join("joiner-epoch-99-avg-1.int8.onnx").to_string_lossy().into_owned());
        config.model_config.tokens =
            Some(dir.join("tokens.txt").to_string_lossy().into_owned());
        config.enable_endpoint = false;
        config.model_config.modeling_unit = modeling_unit.map(|s| s.to_string());
        config.model_config.bpe_vocab = bpe_vocab.map(|p| p.to_string_lossy().into_owned());
        if let Some(hw) = hotwords_file {
            config.decoding_method = Some("modified_beam_search".into());
            config.max_active_paths = 4;
            config.hotwords_file = Some(hw.to_string_lossy().into_owned());
            config.hotwords_score = 2.0;
        } else {
            config.decoding_method = Some("greedy_search".into());
        }
        let rec = OnlineRecognizer::create(&config).expect("create recognizer");
        let mut stream = rec.create_stream();
        let _ = &mut stream;
        // 分块喂，模拟流式
        for chunk in samples.chunks(3200) {
            stream.accept_waveform(16_000, chunk);
            while rec.is_ready(&stream) { rec.decode(&stream); }
        }
        stream.input_finished();
        while rec.is_ready(&stream) { rec.decode(&stream); }
        rec.get_result(&stream).map(|r| r.text).unwrap_or_default()
    }

    #[test]
    #[ignore]
    fn ab_mixed_zh_en() {
        let home = std::env::var("HOME").unwrap();
        let dir = std::path::PathBuf::from(&home).join(".mouseclaw/models/sherpa-zh-en");
        if !dir.join("tokens.txt").exists() {
            eprintln!("SKIP: 模型不在 {}", dir.display());
            return;
        }
        // MC_TEST_WAV 可逗号分隔多个 wav → 拼接（用不同音色合成真·中英 code-switch）
        let wav = std::env::var("MC_TEST_WAV").unwrap_or_else(|_| "/tmp/mc_push.wav".into());
        let mut samples: Vec<f32> = Vec::new();
        for p in wav.split(',') {
            samples.extend(read_wav_i16_mono(std::path::Path::new(p.trim())));
        }
        eprintln!("wav={wav} samples={} ({:.2}s)", samples.len(), samples.len() as f32 / 16000.0);

        let bpe = dir.join("bpe.vocab");
        // 确保 bpe.vocab 在（用内嵌的写一份）
        if !bpe.exists() { std::fs::write(&bpe, BPE_VOCAB).unwrap(); }

        let tmp = std::env::temp_dir();
        let hw_lower = tmp.join("mc_hw_lower.txt");
        let hw_upper = tmp.join("mc_hw_upper.txt");
        // 旧式：英文整词、原样小写、无 bpe（== 修复前行为）
        std::fs::write(&hw_lower, "push\npull\ncommit\nmerge\n").unwrap();
        // 新式：大写，配 bpe（== 修复后行为）
        std::fs::write(&hw_upper, "PUSH\nPULL\nCOMMIT\nMERGE\n").unwrap();

        let a = transcribe_with(&dir, None, None, None, &samples);
        let b = transcribe_with(&dir, None, None, Some(&hw_lower), &samples);
        let c = transcribe_with(&dir, Some("cjkchar+bpe"), Some(&bpe), Some(&hw_upper), &samples);

        eprintln!("\n=== A 无 hotwords (greedy)        : {a}");
        eprintln!("=== B 旧式 小写 hotwords 无 bpe   : {b}");
        eprintln!("=== C 新式 大写 hotwords + bpe    : {c}");
        eprintln!("=== C recased                     : {}\n", crate::vocab::recase_english(&c));
    }

    // ── SenseVoice spike：离线模型，验证转写质量 + 原生标点（use_itn）──────────
    // 依赖 ~/.mouseclaw/models/sense-voice/{model.int8.onnx,tokens.txt} + 一个 16k 单声道 wav。
    //   cargo test --manifest-path src-tauri/Cargo.toml --lib \
    //     transcribe_stream::tests::spike_sensevoice -- --ignored --nocapture
    // MC_TEST_WAV 可逗号分隔多个 wav。打印 SenseVoice vs 现 zipformer 对比。
    #[test]
    #[ignore]
    fn spike_sensevoice() {
        use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineSenseVoiceModelConfig};
        let home = std::env::var("HOME").unwrap();
        let sv_dir = std::path::PathBuf::from(&home).join(".mouseclaw/models/sense-voice");
        let zip_dir = std::path::PathBuf::from(&home).join(".mouseclaw/models/sherpa-zh-en");
        if !sv_dir.join("model.int8.onnx").exists() {
            eprintln!("SKIP: SenseVoice 模型不在 {}", sv_dir.display());
            return;
        }
        let wav = std::env::var("MC_TEST_WAV").unwrap_or_else(|_| "/tmp/mc_push.wav".into());
        let mut samples: Vec<f32> = Vec::new();
        for p in wav.split(',') {
            samples.extend(read_wav_i16_mono(std::path::Path::new(p.trim())));
        }
        eprintln!("wav={wav} samples={} ({:.2}s)", samples.len(), samples.len() as f32 / 16000.0);

        // SenseVoice 离线转写（use_itn=true → 原生标点 + 数字归一）
        let mut cfg = OfflineRecognizerConfig::default();
        cfg.model_config.sense_voice = OfflineSenseVoiceModelConfig {
            model: Some(sv_dir.join("model.int8.onnx").to_string_lossy().into_owned()),
            language: Some("auto".into()),
            use_itn: true,
        };
        cfg.model_config.tokens = Some(sv_dir.join("tokens.txt").to_string_lossy().into_owned());
        cfg.model_config.num_threads = 2;
        let t0 = std::time::Instant::now();
        let rec = OfflineRecognizer::create(&cfg).expect("create SenseVoice recognizer");
        let stream = rec.create_stream();
        stream.accept_waveform(16_000, &samples);
        rec.decode(&stream);
        let sv_text = stream.get_result().map(|r| r.text).unwrap_or_default();
        let sv_ms = t0.elapsed().as_millis();

        // 现 zipformer（带 hotwords + bpe）对比
        let zip = if zip_dir.join("tokens.txt").exists() {
            let bpe = zip_dir.join("bpe.vocab");
            if !bpe.exists() { std::fs::write(&bpe, BPE_VOCAB).ok(); }
            let active = std::path::PathBuf::from(&home).join(".mouseclaw/vocab/active.txt");
            let hw = if active.exists() { Some(active) } else { None };
            transcribe_with(&zip_dir, Some("cjkchar+bpe"), Some(&bpe), hw.as_deref(), &samples)
        } else { "(zipformer 不在)".into() };

        eprintln!("\n=== SenseVoice (use_itn, {sv_ms}ms) : {sv_text}");
        eprintln!("=== 现 zipformer + hotwords        : {zip}\n");
    }

    /// 验证**生产函数** crate::transcribe_sense::transcribe（非内联 spike）端到端可用。
    #[test]
    #[ignore]
    fn sense_production_path() {
        let home = std::env::var("HOME").unwrap();
        if !std::path::PathBuf::from(&home).join(".mouseclaw/models/sense-voice/model.int8.onnx").exists() {
            eprintln!("SKIP: SenseVoice 模型不在");
            return;
        }
        let wav = std::env::var("MC_TEST_WAV").unwrap_or_else(|_| "/tmp/mc_push.wav".into());
        let mut samples: Vec<f32> = Vec::new();
        for p in wav.split(',') {
            samples.extend(read_wav_i16_mono(std::path::Path::new(p.trim())));
        }
        let t = crate::transcribe_sense::transcribe(&samples).expect("production transcribe");
        eprintln!("\n=== transcribe_sense::transcribe → {t}\n");
        assert!(!t.is_empty(), "production path returned empty");
    }
}
