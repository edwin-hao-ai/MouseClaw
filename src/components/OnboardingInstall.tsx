/**
 * Onboarding · Step 7 · 浏览器能力 + Office Use（v0.4.x）
 *
 * 设计文档：docs/prototypes/onboarding-browser-cdp-20260521.html
 *
 * v0.4.x（2026-05-21）：浏览器自动化改成「三选一」，把 CDP「用我的 Chrome」
 * 升为一等 + 默认推荐项 —— 复用本机 Chrome、带用户登录态、不下载，比 agent-browser 轻。
 *   - cdp  → invoke("enable_browser_automation")（browser_bridge::enable：开调试 Chrome + 注册 chrome-devtools-mcp）
 *   - ab   → install_cli("agent-browser")（独立 headless 引擎，进阶）
 *   - none → 不启用（之后随时在托盘开）
 * Office（officecli）保持独立勾选，与浏览器选项无关。
 */
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useT } from "../i18n";

type Target = "agent-browser" | "officecli";
type BrowserChoice = "cdp" | "ab" | "none";

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

const CMD_AB  = "npm i -g agent-browser && agent-browser install";
const CMD_OFF = "curl -fsSL https://raw.githubusercontent.com/iOfficeAI/OfficeCLI/main/install.sh | bash";

export function OnboardingInstall({ onNext }: OnboardingInstallProps) {
  const t = useT();
  const [browserChoice, setBrowserChoice] = useState<BrowserChoice>("cdp");
  const [enabledOff, setEnabledOff] = useState(true);
  const [showCmd, setShowCmd] = useState(false);
  const [started, setStarted] = useState(false);
  const [rows, setRows] = useState<Record<Target, RowState>>({
    "agent-browser": { kind: "idle" },
    "officecli":     { kind: "idle" },
  });
  const [cdpState, setCdpState] = useState<RowState>({ kind: "idle" });
  const unlistenRef = useRef<UnlistenFn | null>(null);

  // 已就绪的能力直接显示 done
  useEffect(() => {
    invoke<CapabilityStatus>("capability_status").then(c => {
      if (c.chrome_cdp) setCdpState({ kind: "done" });
      setRows(prev => ({
        "agent-browser": c.agent_browser ? { kind: "done" } : prev["agent-browser"],
        "officecli":     c.officecli     ? { kind: "done" } : prev["officecli"],
      }));
    }).catch(() => {/* dev mode */});
  }, []);

  // install_cli 的流式进度（只对 agent-browser / officecli）
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
      setRows(prev => ({ ...prev, [target]: { kind: "failed", code: "unknown", message: String(e) } }));
    }
  };

  const enableCdp = async () => {
    setCdpState({ kind: "installing" });
    try { await invoke("enable_browser_automation"); setCdpState({ kind: "done" }); }
    catch (e) { setCdpState({ kind: "failed", code: "unknown", message: String(e) }); }
  };

  const hasWork = browserChoice !== "none" || enabledOff;

  const handleInstall = () => {
    if (!hasWork) { onNext(); return; }
    setStarted(true);
    // 已就绪的不重复装/启用（CDP enable 本身幂等，ab/officecli 重装是浪费）。
    if (browserChoice === "cdp" && cdpState.kind !== "done")             void enableCdp();
    else if (browserChoice === "ab" && rows["agent-browser"].kind !== "done") void kick("agent-browser");
    if (enabledOff && rows["officecli"].kind !== "done") void kick("officecli");
  };

  const isInFlight = (r: RowState) => r.kind === "installing";
  const browserSettled =
    browserChoice === "none"
    || (browserChoice === "cdp" && !isInFlight(cdpState))
    || (browserChoice === "ab"  && !isInFlight(rows["agent-browser"]));
  const officeSettled = !enabledOff || !isInFlight(rows["officecli"]);
  const allHandled = started && browserSettled && officeSettled;

  const goLabel = browserChoice === "cdp"
      ? t("onbinst.go_cdp") + (enabledOff ? t("onbinst.go_suffix_office") : "")
    : browserChoice === "ab"
      ? t("onbinst.go_ab") + (enabledOff ? t("onbinst.go_suffix_office") : "")
      : t("onbinst.go_office_only");

  // 选项卡（cdp / ab / none），started 后不可改、显示状态
  const Opt = ({ value, icon, title, badge, badgeKind, desc, children }: {
    value: BrowserChoice; icon: string; title: string;
    badge?: string; badgeKind?: "rec" | "adv"; desc: string; children?: React.ReactNode;
  }) => {
    const sel = browserChoice === value;
    return (
      <div className={`ob-browse-opt${sel ? " sel" : ""}${started ? " locked" : ""}`}
           onClick={() => { if (!started) setBrowserChoice(value); }}>
        <div className="ob-browse-opt-head">
          <span className="ob-browse-radio" />
          <span className="ob-browse-icon">{icon}</span>
          <span className="ob-browse-opt-title">{title}</span>
          {badge && <span className={`ob-browse-badge ob-browse-badge-${badgeKind}`}>{badge}</span>}
        </div>
        <div className="ob-browse-opt-desc">{desc}</div>
        {children}
      </div>
    );
  };

  const Tags = ({ items }: { items: Array<[string, "good" | "warn" | "info"]> }) => (
    <div className="ob-browse-tags">
      {items.map(([txt, k]) => <span key={txt} className={`ob-browse-tag ob-browse-tag-${k}`}>{txt}</span>)}
    </div>
  );

  // 浏览器选项的进行中 / 完成 / 失败 状态条（cdp 用 cdpState，ab 用 rows）
  const browserStatus = () => {
    if (!started) return null;
    if (browserChoice === "cdp") {
      if (cdpState.kind === "installing")
        return <div className="ob-install-log">{t("onbinst.cdp_enabling")}</div>;
      if (cdpState.kind === "done")
        return <div className="ob-install-log" style={{ color: "#3fa66a" }}>{t("onbinst.cdp_done")}</div>;
      if (cdpState.kind === "failed")
        return <div className="ob-install-fail"><span>⚠️ {cdpState.message}</span>
                 <button className="ob-install-retry" onClick={enableCdp}>{t("status.install.retry")}</button></div>;
    }
    if (browserChoice === "ab") return rowStatus("agent-browser");
    return null;
  };

  const rowStatus = (target: Target) => {
    const r = rows[target];
    if (r.kind === "installing")
      return <><div className="ob-install-bar"><div className="ob-install-bar-fill" /></div>
               <div className="ob-install-log">{r.lastLog ?? t("status.install.starting")}</div></>;
    if (r.kind === "done")
      return <div className="ob-install-log" style={{ color: "#3fa66a" }}>✓ {
        target === "officecli" ? t("status.row.officecli.good") : t("status.row.agentbrowser.good")}</div>;
    if (r.kind === "failed")
      return <div className="ob-install-fail"><span>⚠️ {r.message}</span>
        {r.code === "no-npm"
          ? <a className="ob-install-link" href="https://nodejs.org" target="_blank" rel="noreferrer">{t("onbinst.open_node")}</a>
          : <button className="ob-install-retry" onClick={() => kick(target)}>{t("status.install.retry")}</button>}
      </div>;
    return null;
  };

  return (
    <div className="ob-root">
      <h1 className="ob-title">{t("onbinst.title")}</h1>
      <p className="ob-subtitle">{t("onbinst.lead")}</p>

      <div className="ob-browse-group">{t("onbinst.browser_group")}</div>

      <Opt value="cdp" icon="✨" title={t("onbinst.cdp_title")}
           badge={t("onbinst.badge_rec")} badgeKind="rec" desc={t("onbinst.cdp_desc")}>
        <Tags items={[[t("onbinst.tag_nodl"), "good"], [t("onbinst.tag_login"), "good"], [t("onbinst.tag_light"), "good"]]} />
        <div className="ob-browse-note">{t("onbinst.cdp_note")}</div>
        {browserChoice === "cdp" && browserStatus()}
      </Opt>

      <Opt value="ab" icon="📦" title={t("onbinst.ab_title")}
           badge={t("onbinst.badge_adv")} badgeKind="adv" desc={t("onbinst.ab_desc")}>
        <Tags items={[[t("onbinst.tag_npm"), "info"], [t("onbinst.tag_dl"), "warn"], [t("onbinst.tag_nologin"), "warn"]]} />
        {browserChoice === "ab" && browserStatus()}
      </Opt>

      <Opt value="none" icon="🚫" title={t("onbinst.none_title")} desc={t("onbinst.none_desc")} />

      <div className="ob-browse-group">{t("onbinst.office_group")}</div>
      <div className="ob-browse-office">
        <span className="ob-browse-icon">📄</span>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div className="ob-browse-opt-title" style={{ fontSize: 13.5 }}>{t("status.row.officecli.title")}</div>
          <div className="ob-browse-opt-desc" style={{ margin: "2px 0 0" }}>{t("onbinst.office_meta")}</div>
        </div>
        {!started && rows["officecli"].kind !== "done" && (
          <label className="ob-install-check">
            <input type="checkbox" checked={enabledOff} onChange={e => setEnabledOff(e.target.checked)} />
          </label>
        )}
      </div>
      {started && enabledOff && rowStatus("officecli")}

      {(browserChoice === "ab" || enabledOff) && (
        <>
          <button className="ob-install-show-cmd" onClick={() => setShowCmd(s => !s)}>
            {showCmd ? "▾ " : "▸ "}{t("onbinst.show_cmd")}
          </button>
          {showCmd && (
            <pre className="ob-install-cmd-block">
{browserChoice === "ab" ? CMD_AB + "\n" : ""}{enabledOff ? CMD_OFF : ""}
            </pre>
          )}
        </>
      )}

      {!started ? (
        <>
          <button className="ob-cta" onClick={handleInstall}>
            {hasWork ? goLabel : t("onbinst.continue")}
          </button>
          <button className="ob-cta ob-cta-secondary" onClick={onNext}>{t("onbinst.skip")}</button>
        </>
      ) : (
        <button className={`ob-cta ${allHandled ? "" : "ob-cta-secondary"}`} onClick={onNext}>
          {allHandled ? t("onbinst.continue") : t("onbinst.installing")}
        </button>
      )}
    </div>
  );
}
