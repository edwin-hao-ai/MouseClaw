/**
 * v0.4.0 · 模型下载进度独立窗口（label="downloader", ?view=downloader）
 *
 * 从托盘「📥 模型下载进度」或 blocked 气泡的「点这里看进度」打开。
 * 监听 Rust emit 的 `model-progress` 事件，聚合 ASR + 标点两条模型的下载状态：
 *   - 总进度环（MB / % / ETA / 速度）
 *   - 当前文件 + 当前镜像（fallback 切换时透明展示）
 *   - 错误时显示具体 URL + retry 按钮
 *
 * 设计取向：信息密度大于美化（用户在看这个窗口 = 在等模型，要看到东西在动）。
 */
import { useEffect, useState, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface ProgressEvent {
  model_id: string;
  display: string;
  file_index: number;
  file_total: number;
  current_file: string;
  current_bytes: number;
  current_total: number;
  total_done: number;
  total_expected: number;
  mirror: string;
  phase: "downloading" | "fallback" | "verifying" | "ok" | "error" | string;
  message: string | null;
}

function fmtMb(n: number): string {
  return (n / 1024 / 1024).toFixed(1) + " MB";
}
function shortHost(url: string): string {
  if (!url) return "";
  return url.replace(/^https?:\/\//, "").split("/")[0];
}
function fmtEta(secs: number): string {
  if (!isFinite(secs) || secs <= 0) return "—";
  if (secs > 3600) return ">1 小时";
  const m = Math.floor(secs / 60);
  const s = Math.floor(secs % 60);
  if (m > 0) return `${m} 分 ${s} 秒`;
  return `${s} 秒`;
}

export default function DownloaderView() {
  // 每个 model_id 保存最新的一条 progress
  const [progress, setProgress] = useState<Record<string, ProgressEvent>>({});
  // 速度计算：上一次时间 + 上一次 total_done
  const speedRef = useRef<Record<string, { t: number; bytes: number; mbps: number }>>({});
  const [, force] = useState(0);

  useEffect(() => {
    // v0.4.0 · 挂载时主动拉一次当前状态 —— 应对模型已就绪（is_ready）时
    // kick_off_download_if_missing 早退、没 emit 任何 event 的场景。
    invoke<ProgressEvent[]>("get_model_status").then(list => {
      if (list && list.length > 0) {
        const map: Record<string, ProgressEvent> = {};
        for (const e of list) map[e.model_id] = e;
        setProgress(prev => ({ ...prev, ...map }));
      }
    }).catch(err => console.warn("get_model_status:", err));

    const un = listen<ProgressEvent>("model-progress", (e) => {
      setProgress(prev => ({ ...prev, [e.payload.model_id]: e.payload }));
      // 更新速度
      const now = Date.now();
      const prevSpeed = speedRef.current[e.payload.model_id];
      if (prevSpeed && now - prevSpeed.t > 500) {
        const dt = (now - prevSpeed.t) / 1000;
        const db = e.payload.total_done - prevSpeed.bytes;
        const mbps = db / dt / 1024 / 1024;
        speedRef.current[e.payload.model_id] = { t: now, bytes: e.payload.total_done, mbps };
      } else if (!prevSpeed) {
        speedRef.current[e.payload.model_id] = { t: now, bytes: e.payload.total_done, mbps: 0 };
      }
      force(x => x + 1);
    });
    return () => { un.then(fn => fn()).catch(() => {}); };
  }, []);

  const entries = Object.values(progress);
  const allDone = entries.length > 0 && entries.every(p => p.phase === "ok");
  const anyError = entries.some(p => p.phase === "error");

  // v0.4.0 · 模型全下完 → 检查是否已做过引导，没做过自动触发 tour
  const tourTriggered = useRef(false);
  useEffect(() => {
    if (allDone && !tourTriggered.current) {
      tourTriggered.current = true;
      // 等 2 秒让用户合上下载窗口
      setTimeout(() => {
        invoke<boolean>("get_tour_done").then(done => {
          if (!done) invoke("tour_start").catch(() => {});
        }).catch(() => {});
      }, 2000);
    }
  }, [allDone]);

  return (
    <div style={{
      // v0.4.0 · flex layout: header 固定 + content 可滚 + sticky 底栏。
      // 之前 minHeight + padding 让长内容把「🔁 重试」按钮挤出 460px 视口
      display: "flex",
      flexDirection: "column",
      height: "100vh",
      fontFamily: "-apple-system, 'PingFang SC', system-ui, sans-serif",
      color: "#2b2622",
      background: "#faf7f2",
    }}>
      <div style={{ padding: "20px 24px 0", flexShrink: 0 }}>
      <h1 style={{ fontSize: 18, fontWeight: 700, margin: "0 0 4px" }}>
        🦞 MouseClaw 正在准备语音模型
      </h1>
      <p style={{ fontSize: 13, color: "#7a7167", margin: "0 0 16px" }}>
        首次启动需要下载 sherpa-onnx 语音识别 + 标点模型 ——
        国内会优先用 hf-mirror.com / gh-proxy.com，无需 VPN。
        </p>
      </div>

      <div style={{ flex: 1, overflowY: "auto", padding: "0 24px 16px" }}>
      {entries.length === 0 && (
        <div style={{ color: "#7a7167", fontSize: 13 }}>等待开始…</div>
      )}

      {entries.map(p => {
        const pct = p.total_expected > 0 ? (p.total_done / p.total_expected * 100) : 0;
        const speed = speedRef.current[p.model_id]?.mbps ?? 0;
        const eta = speed > 0 ? (p.total_expected - p.total_done) / 1024 / 1024 / speed : Infinity;
        const phaseLabel = ({
          downloading: "📥 下载中",
          fallback: "🔁 切镜像",
          verifying: "🔍 校验中",
          ok: "✅ 完成",
          error: "❌ 失败",
        } as Record<string, string>)[p.phase] || p.phase;

        return (
          <div key={p.model_id} style={{
            marginBottom: 20,
            padding: 16,
            background: "#fff",
            border: "1px solid #e8e2d8",
            borderRadius: 12,
          }}>
            <div style={{ display: "flex", justifyContent: "space-between", marginBottom: 8 }}>
              <span style={{ fontWeight: 600 }}>{p.display}</span>
              <span style={{ fontSize: 12, color: "#7a7167" }}>{phaseLabel}</span>
            </div>

            {/* 进度条 */}
            <div style={{
              height: 10,
              background: "#f0eae0",
              borderRadius: 5,
              overflow: "hidden",
              marginBottom: 8,
            }}>
              <div style={{
                height: "100%",
                width: `${pct}%`,
                background: p.phase === "error" ? "#e85a5a" :
                  p.phase === "ok" ? "#5ed091" : "#e8638c",
                transition: "width 200ms ease, background 200ms ease",
              }} />
            </div>

            <div style={{ fontSize: 12, color: "#5a5249", display: "flex", gap: 14, flexWrap: "wrap" }}>
              <span>{fmtMb(p.total_done)} / {fmtMb(p.total_expected)}</span>
              <span>{pct.toFixed(1)} %</span>
              {p.phase === "downloading" && speed > 0 && (
                <>
                  <span>速度 {speed.toFixed(1)} MB/s</span>
                  <span>剩余 {fmtEta(eta)}</span>
                </>
              )}
            </div>

            {(p.phase === "downloading" || p.phase === "fallback") && (
              <div style={{ fontSize: 11, color: "#a39888", marginTop: 6 }}>
                文件 {p.file_index + 1}/{p.file_total}
                {p.current_file && <>: <code>{p.current_file}</code></>}
                {p.mirror && <span> · 镜像 {shortHost(p.mirror)}</span>}
              </div>
            )}

            {p.message && (
              <div style={{
                fontSize: 12,
                marginTop: 8,
                padding: "6px 10px",
                background: p.phase === "error" ? "#ffeae8" :
                  p.phase === "fallback" ? "#fff5dd" : "#f3fbf6",
                borderRadius: 6,
                color: p.phase === "error" ? "#b13838" : "#5a5249",
              }}>
                {p.message}
              </div>
            )}
          </div>
        );
      })}
      </div>

      {/* sticky 底栏 —— 完成提示 + 重试按钮永远在视口 */}
      {(allDone || anyError) && (
        <div style={{
          flexShrink: 0,
          padding: "12px 24px 16px",
          background: "#faf7f2",
          borderTop: "1px solid #ece6dd",
        }}>
          {allDone && (
            <div style={{
              padding: 14,
              background: "#e8f8ed",
              border: "1px solid #b8e8c3",
              borderRadius: 10,
              fontSize: 13,
              color: "#1a6b3a",
            }}>
              🎉 全部就绪！现在可以按全局快捷键开始用语音了。
            </div>
          )}

          {anyError && (
            <button
              type="button"
              style={{
                width: "100%",
                padding: "12px 16px",
                background: "#e8638c", color: "#fff",
                border: 0, borderRadius: 8, cursor: "pointer",
                fontSize: 14, fontWeight: 600,
              }}
              onClick={() => invoke("retry_model_downloads").catch(e => alert(String(e)))}
            >
              🔁 重试下载（之前下到一半的会从断点继续）
            </button>
          )}
        </div>
      )}
    </div>
  );
}
