/**
 * Onboarding · Step 7 · 装上 Browser Use / Office Use CLI（v0.4.x）
 *
 * 设计文档：docs/prototypes/auto-install-cli-20260520.html · State 1-5
 *
 * UX 关键：
 *   - 默认勾选两项，但允许取消 / 跳过
 *   - 「显示完整命令」展开 —— 不藏 curl|bash / npm i -g 全文
 *   - 并行装，进度条 + 流式 log（最新一行）
 *   - 失败兜底：没 npm → 引导 nodejs.org；网络断 → 重试 / 复制命令 / 打开 releases
 *   - 装完那行变绿，所有都装完 / 都跳过 → onNext
 */
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useT } from "../i18n";

type Target = "agent-browser" | "officecli";

type RowState =
  | { kind: "idle" }
  | { kind: "installing"; lastLog?: string }
  | { kind: "done" }
  | { kind: "failed"; code: string; message: string };

interface CapabilityStatus {
  claude_cli: boolean;
  agent_browser: boolean;
  chrome_cdp: boolean;
  officecli: boolean;
}

interface OnboardingInstallProps {
  onNext: () => void;
}

const CMD_AB  = "npm i -g @vercel/agent-browser";
const CMD_OFF = "curl -fsSL https://raw.githubusercontent.com/iOfficeAI/OfficeCLI/main/install.sh | bash";

export function OnboardingInstall({ onNext }: OnboardingInstallProps) {
  const t = useT();
  const [enabledAB,  setEnabledAB]  = useState(true);
  const [enabledOff, setEnabledOff] = useState(true);
  const [showCmd, setShowCmd] = useState(false);
  const [started, setStarted] = useState(false);
  const [rows, setRows] = useState<Record<Target, RowState>>({
    "agent-browser": { kind: "idle" },
    "officecli":     { kind: "idle" },
  });
  const unlistenRef = useRef<UnlistenFn | null>(null);

  // 先看本机已有的状态 —— 已装就直接显示绿色 done
  useEffect(() => {
    invoke<CapabilityStatus>("capability_status").then(c => {
      setRows(prev => ({
        "agent-browser": c.agent_browser ? { kind: "done" } : prev["agent-browser"],
        "officecli":     c.officecli     ? { kind: "done" } : prev["officecli"],
      }));
    }).catch(() => {/* dev mode */});
  }, []);

  // 订阅 install-progress
  useEffect(() => {
    let active = true;
    (async () => {
      type Ev =
        | { phase: "started"; target: string }
        | { phase: "log"; target: string; line: string }
        | { phase: "done"; target: string }
        | { phase: "failed"; target: string; code: string; message: string };
      try {
        const un = await listen<Ev>("install-progress", (e) => {
          if (!active) return;
          const p = e.payload;
          const tgt = p.target as Target;
          if (tgt !== "agent-browser" && tgt !== "officecli") return;
          setRows(prev => {
            const next = { ...prev };
            if (p.phase === "started")  next[tgt] = { kind: "installing" };
            else if (p.phase === "log") {
              const cur = next[tgt];
              next[tgt] = { kind: "installing",
                            lastLog: p.line || (cur.kind === "installing" ? cur.lastLog : undefined) };
            }
            else if (p.phase === "done")   next[tgt] = { kind: "done" };
            else if (p.phase === "failed") next[tgt] = { kind: "failed", code: p.code, message: p.message };
            return next;
          });
        });
        unlistenRef.current = un;
      } catch {/* dev mode */}
    })();
    return () => {
      active = false;
      if (unlistenRef.current) unlistenRef.current();
    };
  }, []);

  const kick = async (target: Target) => {
    setRows(prev => ({ ...prev, [target]: { kind: "installing" } }));
    try { await invoke("install_cli", { target }); }
    catch (e) {
      setRows(prev => ({
        ...prev,
        [target]: { kind: "failed", code: "unknown", message: String(e) },
      }));
    }
  };

  const handleInstall = () => {
    setStarted(true);
    if (enabledAB)  void kick("agent-browser");
    if (enabledOff) void kick("officecli");
  };

  const isInFlight = (r: RowState) => r.kind === "installing";
  const allSettled =
    !isInFlight(rows["agent-browser"]) && !isInFlight(rows["officecli"]);
  const allHandled = started && allSettled;

  const renderRow = (
    target: Target, icon: string, title: string, meta: string,
    enabled: boolean, setEnabled: (b: boolean) => void,
  ) => {
    const r = rows[target];
    const dotColor = r.kind === "done" ? "#3fa66a"
      : r.kind === "installing" ? "#d9952b"
      : r.kind === "failed" ? "#b8302c" : "#c8c8d0";
    return (
      <div className="ob-install-row" key={target}>
        <div className="ob-install-row-head">
          <span className="ob-install-icon">{icon}</span>
          <div style={{ flex: 1, minWidth: 0 }}>
            <div className="ob-install-name">{title}</div>
            <div className="ob-install-meta">{meta}</div>
          </div>
          {!started && r.kind !== "done" && (
            <label className="ob-install-check">
              <input type="checkbox" checked={enabled}
                     onChange={e => setEnabled(e.target.checked)} />
            </label>
          )}
          <span style={{
            width: 10, height: 10, borderRadius: 999,
            background: dotColor, marginLeft: 8,
          }} />
        </div>
        {r.kind === "installing" && (
          <>
            <div className="ob-install-bar"><div className="ob-install-bar-fill" /></div>
            <div className="ob-install-log">{r.lastLog ?? t("status.install.starting")}</div>
          </>
        )}
        {r.kind === "done" && (
          <div className="ob-install-log" style={{ color: "#3fa66a" }}>
            ✓ {target === "officecli" ? t("status.row.officecli.good") : t("status.row.agentbrowser.good")}
          </div>
        )}
        {r.kind === "failed" && (
          <div className="ob-install-fail">
            <span>⚠️ {r.message}</span>
            {r.code === "no-npm" ? (
              <a className="ob-install-link"
                 href="https://nodejs.org" target="_blank" rel="noreferrer">
                {t("onbinst.open_node")}
              </a>
            ) : (
              <button className="ob-install-retry" onClick={() => kick(target)}>
                {t("status.install.retry")}
              </button>
            )}
          </div>
        )}
      </div>
    );
  };

  return (
    <div className="ob-root">
      <div className="ob-mouse-stage">
        {/* 任意活泼态都行 —— think 表示"准备中" */}
      </div>
      <h1 className="ob-title">{t("onbinst.title")}</h1>
      <p className="ob-subtitle">{t("onbinst.lead")}</p>

      <div className="ob-install-list">
        {renderRow(
          "agent-browser", "🌐",
          t("status.row.agentbrowser.title"),
          "@vercel · " + t("status.row.agentbrowser.bad"),
          enabledAB, setEnabledAB,
        )}
        {renderRow(
          "officecli", "📄",
          t("status.row.officecli.title"),
          "@iOfficeAI · " + t("status.row.officecli.bad"),
          enabledOff, setEnabledOff,
        )}
      </div>

      <button className="ob-install-show-cmd"
              onClick={() => setShowCmd(s => !s)}>
        {showCmd ? "▾ " : "▸ "}{t("onbinst.show_cmd")}
      </button>
      {showCmd && (
        <pre className="ob-install-cmd-block">
{enabledAB ? CMD_AB + "\n" : ""}{enabledOff ? CMD_OFF : ""}
        </pre>
      )}

      {!started ? (
        <>
          <button className="ob-cta"
                  disabled={!enabledAB && !enabledOff}
                  onClick={handleInstall}>
            {t("onbinst.go")}
          </button>
          <button className="ob-cta ob-cta-secondary" onClick={onNext}>
            {t("onbinst.skip")}
          </button>
        </>
      ) : (
        <button className={`ob-cta ${allHandled ? "" : "ob-cta-secondary"}`}
                onClick={onNext}>
          {allHandled ? t("onbinst.continue") : t("onbinst.installing")}
        </button>
      )}
    </div>
  );
}
