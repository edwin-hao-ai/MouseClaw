/**
 * Six-step Onboarding (v0.1.27):
 *   Step 1 — pick a global shortcut (AI summon)
 *   Step 2 — pick an AI backend
 *   Step 3 — pick a desktop pet skin
 *   Step 4 — pick voice IME trigger key
 *   Step 5 — pick pet anchor / where the pet lives (NEW v0.1.27)
 *   Step 6 — grant required permissions
 */
import { useState, useEffect, useCallback, type CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import { PixelMouse } from "./PixelMouse";
import type { BackendChoice, SkinId, PetAnchor } from "../types";
import { SKINS, DEFAULT_SKIN } from "../skins";
import { useT } from "../i18n";
import "./Onboarding.css";

export type ShortcutChoice =
  | "double-option"
  | "hold-option"
  | "double-cmd"
  | "hold-cmd";

interface PermissionStatus {
  accessibility: boolean;
  screen_recording: boolean;
  microphone: boolean;
}

/** voice IME 触发键的 id —— 必须跟 Rust ImeTrigger::as_str() 对齐 */
export type VoiceImeTrigger =
  | "fn" | "option" | "control"
  | "right-shift" | "right-command" | "right-option"
  | "disabled"; // 用户选不启用

/** v0.4.0 · 语音识别用的模型语言。"zh" = 中文（含中英混合）/ "en" = English-only */
export type VoiceLang = "zh" | "en";

interface OnboardingProps {
  onComplete: (
    choice: ShortcutChoice,
    backend: BackendChoice,
    skin: SkinId,
    voiceImeTrigger: VoiceImeTrigger,
    petAnchor: PetAnchor,
    voiceLang: VoiceLang,
  ) => void;
}

// OPTIONS / BACKENDS / PERMISSIONS — 函数化，每次渲染时按当前语言重建
function buildOptions(t: ReturnType<typeof useT>) {
  return [
    { id: "hold-option" as ShortcutChoice,
      label: t("onboarding.shortcut.hint").includes("Hold") ? "Hold ⌃ + ⌘ + Space" : "按住 ⌃ + ⌘ + 空格",
      keyHint: "⌃ ⌘ Space", tag: t("onboarding.shortcut.zero_conflict") },
    { id: "hold-cmd" as ShortcutChoice,
      label: t("onboarding.shortcut.hint").includes("Hold") ? "Hold ⌃ + ⌘ + M" : "按住 ⌃ + ⌘ + M",
      keyHint: "⌃ ⌘ M" },
  ];
}

const BACKENDS_META: Array<{ id: BackendChoice; label: string; descKey: string; tag?: string }> = [
  { id: "claude-cli", label: "Claude Code CLI", descKey: "claude", tag: "common.recommended" },
  { id: "codex-cli",  label: "OpenAI Codex CLI", descKey: "codex" },
  { id: "openclaw-cli", label: "OpenClaw CLI",  descKey: "openclaw" },
  { id: "hermes-agent", label: "Hermes Agent (Nous Research)", descKey: "hermes" },
];

function backendDesc(id: BackendChoice, t: ReturnType<typeof useT>): string {
  // 这块描述不上升到 i18n key 表（太琐碎），直接走双语 inline
  const en = t("onboarding.shortcut.hint").includes("Hold");
  switch (id) {
    case "claude-cli":
      return en
        ? "Most mature · native agentic + image reading. Needs claude installed & logged in."
        : "最成熟 · 原生 agentic + 读图。需已装并登录 claude。";
    case "codex-cli":
      return en
        ? "codex exec non-interactive mode. Needs npm i -g @openai/codex + OpenAI key."
        : "codex exec 非交互模式。需 npm i -g @openai/codex 并配好 key。";
    case "openclaw-cli":
      return en
        ? "openclaw agent --local. Needs npm i -g openclaw + provider key in shell."
        : "openclaw agent --local。需 npm i -g openclaw 并配好 provider key。";
    case "hermes-agent":
      return en
        ? "Hermes -z one-shot mode. Self-improving agent from Nous Research. Needs hermes installed + setup."
        : "hermes -z 单次模式。Nous Research 的自学习 agent。需先装 hermes 并配 provider key。";
  }
}

function buildPermissions(t: ReturnType<typeof useT>) {
  return [
    { key: "accessibility" as const, icon: "⌨️",
      title: t("perms.accessibility.title"), desc: t("perms.accessibility.desc") },
    { key: "screen_recording" as const, icon: "🖥️",
      title: t("perms.screen.title"), desc: t("perms.screen.desc") },
    { key: "microphone" as const, icon: "🎙️",
      title: t("perms.mic.title"), desc: t("perms.mic.desc") },
  ];
}

export function Onboarding({ onComplete }: OnboardingProps) {
  const t = useT();
  const OPTIONS = buildOptions(t);
  const PERMISSIONS = buildPermissions(t);
  const [step, setStep] = useState<1 | 2 | 3 | 4 | 5 | 6>(1);
  const [selected, setSelected] = useState<ShortcutChoice>("hold-option");
  const [backend, setBackend] = useState<BackendChoice>("claude-cli");
  const [skin, setSkin] = useState<SkinId>(DEFAULT_SKIN);
  const [voiceImeTrigger, setVoiceImeTrigger] = useState<VoiceImeTrigger>("fn");
  // v0.4.0 · 语音识别模型语言（zh / en）。决定下哪个 sherpa 模型 (~199MB vs ~73MB)。
  // 用户切到 en：仅识别英文，但准确率明显高于 zh-en 双语模型上的纯英文。
  // 默认按 UI 语言推断 —— UI 是中文 → zh；UI 是英文 → en。
  const [voiceLang, setVoiceLang] = useState<VoiceLang>(() => {
    // 从 UI 语言推一个合理默认。t() 拿不到，简单判一下浏览器 lang
    const ui = navigator.language?.toLowerCase() ?? "";
    return ui.startsWith("zh") ? "zh" : "en";
  });
  // v0.1.27 · 默认 bottom-right —— 最不挡视线
  const [petAnchor, setPetAnchor] = useState<PetAnchor>("bottom-right");
  // v0.1.28 · 后端 CLI 安装状态（id → {installed, installCmd, installUrl}）
  const [backendStatus, setBackendStatus] = useState<
    Record<string, { installed: boolean; installCmd: string; installUrl: string } | "loading">
  >({});
  // v0.1.26 · 开机自启动 —— 进 step 5 时拉一次系统真实状态，用户切换调 set_autostart
  const [autostart, setAutostart] = useState<boolean>(true);
  useEffect(() => {
    invoke<boolean>("get_autostart").then(setAutostart).catch(() => setAutostart(true));
  }, []);
  const [perms, setPerms] = useState<PermissionStatus>({
    accessibility: false,
    screen_recording: false,
    microphone: false,
  });
  // 用户点过「去开启」的权限。屏幕录制授权后本进程查不到（macOS 设计 ——
  // 必须重启 app 才生效），所以靠「点过请求」+ 重启来兜。
  const [requested, setRequested] = useState<Set<keyof PermissionStatus>>(new Set());

  // 轮询权限状态（accessibility / microphone 授权后会实时变绿；
  // screen_recording 不会 —— 需重启，所以用 requested 集合兜底）
  const refreshPerms = useCallback(async () => {
    try {
      const status = await invoke<PermissionStatus>("check_permissions");
      setPerms(status);
    } catch {
      // 在浏览器 dev 模式下 invoke 不可用，默认全部 true
      setPerms({ accessibility: true, screen_recording: true, microphone: true });
    }
  }, []);

  useEffect(() => {
    if (step !== 6) return;
    refreshPerms();
    // 每 1.5 秒刷新一次，让用户授权后立即看到变化
    const timer = setInterval(refreshPerms, 1500);
    return () => clearInterval(timer);
  }, [step, refreshPerms]);

  // v0.1.28 · 进 step 2 时并发检测 4 个 backend CLI 是否装在 PATH
  useEffect(() => {
    if (step !== 2) return;
    const ids: BackendChoice[] = ["claude-cli", "codex-cli", "openclaw-cli", "hermes-agent"];
    // mark all as loading first so UI doesn't flicker
    setBackendStatus(Object.fromEntries(ids.map(id => [id, "loading" as const])));
    ids.forEach((id) => {
      invoke<{ installed: boolean; installCmd: string; installUrl: string }>(
        "check_backend_installed", { backend: id }
      ).then((res) => {
        setBackendStatus(prev => ({ ...prev, [id]: res }));
      }).catch(() => {
        // browser-only mode 或 invoke 失败 —— 当成 "已装" 不挡用户
        setBackendStatus(prev => ({
          ...prev,
          [id]: { installed: true, installCmd: "", installUrl: "" },
        }));
      });
    });
  }, [step]);

  // 一个权限算"搞定" = 真的查到已授权 OR (是屏幕录制 且 已点过请求)
  const isDone = (key: keyof PermissionStatus): boolean => {
    if (perms[key]) return true;
    if (key === "screen_recording" && requested.has(key)) return true;
    return false;
  };

  // 三项都搞定才能完成。完成时会重启 app（让屏幕录制生效）。
  const allDone =
    isDone("accessibility") && isDone("screen_recording") && isDone("microphone");

  const handleOpenPref = async (key: keyof PermissionStatus) => {
    setRequested((prev) => new Set(prev).add(key));
    try {
      await invoke("request_permission", { name: key });
    } catch {
      // browser dev mode
    }
  };

  // ── Step 1: 选快捷键 ──────────────────────────────────────────────────────
  if (step === 1) {
    return (
      <div className="ob-root">
        <div className="ob-mouse-stage">
          <PixelMouse state="listen" size={96} skin={skin} />
        </div>
        <h1 className="ob-title">{t("onboarding.welcome.title")}</h1>
        <p className="ob-subtitle">{t("onboarding.welcome.subtitle")}</p>
        <div className="ob-options" role="radiogroup" aria-label="选择触发快捷键">
          {OPTIONS.map(opt => (
            <button
              key={opt.id}
              type="button"
              role="radio"
              aria-checked={selected === opt.id}
              className={`ob-option ${selected === opt.id ? "selected" : ""}`}
              onClick={() => setSelected(opt.id)}
            >
              <span className="ob-key">{opt.keyHint}</span>
              <span className="ob-label">{opt.label}</span>
              {opt.tag && <span className="ob-tag">{opt.tag}</span>}
            </button>
          ))}
        </div>
        <button
          type="button"
          className="ob-cta"
          onClick={() => setStep(2)}
        >
          {t("onboarding.cta.next_backend")}
        </button>
      </div>
    );
  }

  // ── Step 2: 选 AI 后端 ────────────────────────────────────────────────────
  if (step === 2) {
    return (
      <div className="ob-root">
        <div className="ob-mouse-stage">
          <PixelMouse state="think" size={96} skin={skin} />
        </div>
        <h1 className="ob-title">{t("onboarding.backend.title")}</h1>
        <p className="ob-subtitle">
          {t("onboarding.backend.subtitle")}<br />
          {t("onboarding.backend.uncertain_hint")}
        </p>
        <div className="ob-options" role="radiogroup" aria-label={t("onboarding.backend.title")}>
          {BACKENDS_META.map(b => {
            const st = backendStatus[b.id];
            const isLoading = st === "loading";
            const stObj = (st && st !== "loading") ? st : null;
            const installed = stObj?.installed === true;
            const missing = stObj?.installed === false;
            return (
              <button
                key={b.id}
                type="button"
                role="radio"
                aria-checked={backend === b.id}
                className={`ob-option ${backend === b.id ? "selected" : ""}`}
                onClick={() => setBackend(b.id)}
              >
                <span className="ob-label">
                  {b.label}
                  {isLoading && (
                    <span className="ob-install-pill ob-install-loading">…</span>
                  )}
                  {installed && (
                    <span className="ob-install-pill ob-install-ok">{t("backend.installed")}</span>
                  )}
                  {missing && (
                    <span className="ob-install-pill ob-install-missing">{t("backend.missing")}</span>
                  )}
                </span>
                <span className="ob-perm-desc">{backendDesc(b.id, t)}</span>
                {missing && stObj && (
                  <div
                    className="ob-install-hint"
                    onClick={(e) => e.stopPropagation()} // 别冒泡到 button 触发 select
                  >
                    <code className="ob-install-cmd">{stObj.installCmd}</code>
                    <a
                      href={stObj.installUrl}
                      target="_blank"
                      rel="noreferrer"
                      className="ob-install-link"
                    >
                      {t("backend.install_open")} ↗
                    </a>
                  </div>
                )}
                {b.tag && <span className="ob-tag">{t(b.tag as "common.recommended")}</span>}
              </button>
            );
          })}
        </div>
        <button
          type="button"
          className="ob-cta"
          onClick={() => setStep(3)}
        >
          {t("onboarding.cta.next_skin")}
        </button>
      </div>
    );
  }

  // ── Step 3: 挑桌宠 ────────────────────────────────────────────────────────
  if (step === 3) {
    return (
      <div className="ob-root">
        <div className="ob-mouse-stage">
          <PixelMouse state="listen" size={96} skin={skin} />
        </div>
        <h1 className="ob-title">{t("onboarding.skin.title")}</h1>
        <p className="ob-subtitle">{t("onboarding.skin.subtitle")}</p>
        <div
          className="ob-options"
          role="radiogroup"
          aria-label={t("onboarding.skin.title")}
          style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: 10 }}
        >
          {SKINS.map(s => (
            <button
              key={s.id}
              type="button"
              role="radio"
              aria-checked={skin === s.id}
              className={`ob-option ${skin === s.id ? "selected" : ""}`}
              onClick={() => setSkin(s.id)}
              style={{ flexDirection: "column", alignItems: "center", padding: 12, minHeight: 130 }}
            >
              <div style={{ marginBottom: 6 }}>
                <PixelMouse state="listen" size={64} skin={s.id} />
              </div>
              <span className="ob-label" style={{ textAlign: "center" }}>{s.name}</span>
              {s.tag && <span className="ob-tag">{s.tag}</span>}
            </button>
          ))}
        </div>
        <button
          type="button"
          className="ob-cta"
          onClick={() => setStep(4)}
        >
          {t("onboarding.cta.next_voice_ime")}
        </button>
      </div>
    );
  }

  // ── Step 4: 选 voice IME 触发键 (NEW v0.1.12) + voice lang (v0.4.0) ───────
  if (step === 4) {
    const triggers: VoiceImeTrigger[] = [
      "fn", "option", "control", "right-shift", "right-command", "right-option", "disabled"
    ];
    const isZhUi = t("onboarding.voice_ime.title").includes("语音");
    return (
      <div className="ob-root">
        <div className="ob-mouse-stage">
          <PixelMouse state="listen" size={96} skin={skin} />
        </div>
        <h1 className="ob-title">{isZhUi ? "语音识别设置" : "Voice Setup"}</h1>
        <p className="ob-subtitle">
          {isZhUi
            ? "先选语音识别模型 —— 首次启动会自动下载（无需手动操作）"
            : "Pick a voice model — auto-downloaded on first launch"}
        </p>

        {/* v0.4.0 · 语音模型语言（决定下哪个 sherpa 模型） */}
        <div style={{ marginBottom: 18 }}>
          <div style={{ fontSize: 13, fontWeight: 600, marginBottom: 8, color: "#5a5249" }}>
            {isZhUi ? "🎤 语音识别语言" : "🎤 Voice model"}
          </div>
          <div className="ob-options" role="radiogroup" aria-label="voice lang">
            <button
              type="button" role="radio"
              aria-checked={voiceLang === "zh"}
              className={`ob-option ${voiceLang === "zh" ? "selected" : ""}`}
              onClick={() => setVoiceLang("zh")}
            >
              <span className="ob-label">
                {isZhUi ? "中文（含中英混合 · ~199MB）" : "Chinese + mixed zh-en (~199MB)"}
              </span>
              {isZhUi && <span className="ob-tag">{t("common.recommended")}</span>}
            </button>
            <button
              type="button" role="radio"
              aria-checked={voiceLang === "en"}
              className={`ob-option ${voiceLang === "en" ? "selected" : ""}`}
              onClick={() => setVoiceLang("en")}
            >
              <span className="ob-label">
                {isZhUi ? "English（纯英文 · ~73MB）" : "English-only (~73MB · best accuracy)"}
              </span>
              {!isZhUi && <span className="ob-tag">{t("common.recommended")}</span>}
            </button>
          </div>
        </div>

        {/* voice IME 触发键 */}
        <div style={{ fontSize: 13, fontWeight: 600, marginBottom: 8, color: "#5a5249" }}>
          {isZhUi ? "⌨️ 语音输入法触发键" : "⌨️ Voice IME hold key"}
        </div>
        <div className="ob-options" role="radiogroup" aria-label={t("onboarding.voice_ime.title")}>
          {triggers.map(tr => (
            <button
              key={tr}
              type="button"
              role="radio"
              aria-checked={voiceImeTrigger === tr}
              className={`ob-option ${voiceImeTrigger === tr ? "selected" : ""}`}
              onClick={() => setVoiceImeTrigger(tr)}
            >
              <span className="ob-label">
                {tr === "disabled"
                  ? t("onboarding.voice_ime.disabled")
                  : t(`vime.trigger.${tr}` as `vime.trigger.fn`)}
              </span>
              {tr === "fn" && <span className="ob-tag">{t("common.recommended")}</span>}
            </button>
          ))}
        </div>
        <p className="ob-skip-hint">{t("onboarding.voice_ime.tip")}</p>
        <button
          type="button"
          className="ob-cta"
          onClick={() => setStep(5)}
        >
          {t("onboarding.cta.next_anchor")}
        </button>
      </div>
    );
  }

  // ── Step 5: 桌宠悬停位置 (NEW v0.1.27) ────────────────────────────────────
  if (step === 5) {
    const corners: { id: PetAnchor; labelKey: "anchor.top-left" | "anchor.top-right"
                       | "anchor.bottom-left" | "anchor.bottom-right" | "anchor.follow"
                       | "anchor.hidden";
                     visual: "tl" | "tr" | "bl" | "br" | "follow" | "hidden";
                     tag?: string }[] = [
      { id: "top-left",     labelKey: "anchor.top-left",     visual: "tl" },
      { id: "top-right",    labelKey: "anchor.top-right",    visual: "tr" },
      { id: "bottom-left",  labelKey: "anchor.bottom-left",  visual: "bl" },
      { id: "bottom-right", labelKey: "anchor.bottom-right", visual: "br",
        tag: t("common.recommended") },
      { id: "follow",       labelKey: "anchor.follow",       visual: "follow" },
      { id: "hidden",       labelKey: "anchor.hidden",       visual: "hidden" },
    ];
    return (
      <div className="ob-root">
        <div className="ob-mouse-stage">
          <PixelMouse state="listen" size={96} skin={skin} />
        </div>
        <h1 className="ob-title">{t("onboarding.anchor.title")}</h1>
        <p className="ob-subtitle">{t("onboarding.anchor.subtitle")}</p>
        <div
          className="ob-options"
          role="radiogroup"
          aria-label={t("onboarding.anchor.title")}
          style={{ display: "grid", gridTemplateColumns: "repeat(6, 1fr)", gap: 8 }}
        >
          {corners.map(c => (
            <button
              key={c.id}
              type="button"
              role="radio"
              aria-checked={petAnchor === c.id}
              className={`ob-option ${petAnchor === c.id ? "selected" : ""}`}
              onClick={() => setPetAnchor(c.id)}
              style={{ flexDirection: "column", alignItems: "center", padding: 10, minHeight: 130 }}
            >
              <AnchorPreview pos={c.visual} skin={skin} />
              <span className="ob-label" style={{ textAlign: "center", marginTop: 6, fontSize: 12 }}>
                {t(c.labelKey)}
              </span>
              {c.tag && <span className="ob-tag">{c.tag}</span>}
            </button>
          ))}
        </div>
        {petAnchor === "follow" && (
          <p className="ob-skip-hint">{t("anchor.follow_hint")}</p>
        )}
        {petAnchor === "hidden" && (
          <p className="ob-skip-hint">{t("anchor.hidden_hint")}</p>
        )}
        <button
          type="button"
          className="ob-cta"
          onClick={() => setStep(6)}
        >
          {t("onboarding.cta.next_perms")}
        </button>
      </div>
    );
  }

  // ── Step 6: 权限申请 ──────────────────────────────────────────────────────
  return (
    <div className="ob-root">
      <div className="ob-mouse-stage">
        <PixelMouse state={allDone ? "jump" : "think"} size={96} skin={skin} />
      </div>
      <h1 className="ob-title">{t("onboarding.perms.title")}</h1>
      <p className="ob-subtitle">{t("onboarding.perms.subtitle")}</p>

      <div className="ob-perm-list">
        {PERMISSIONS.map(p => {
          const granted = perms[p.key];
          const done = isDone(p.key);
          // 屏幕录制：点过请求但还没查到 → "已请求"中间态
          const pendingRestart =
            p.key === "screen_recording" && requested.has(p.key) && !granted;
          return (
            <div
              key={p.key}
              className={`ob-perm-row ${done ? "granted" : ""}`}
            >
              <span className="ob-perm-icon">{p.icon}</span>
              <div className="ob-perm-text">
                <span className="ob-perm-title">{p.title}</span>
                <span className="ob-perm-desc">{p.desc}</span>
              </div>
              {granted ? (
                <span className="ob-perm-check" aria-label={t("onboarding.perms.granted")}>✓</span>
              ) : pendingRestart ? (
                <span className="ob-perm-pending" aria-label={t("onboarding.perms.requested_restart")}>
                  {t("onboarding.perms.requested_restart")}
                </span>
              ) : (
                <button
                  type="button"
                  className="ob-perm-btn"
                  onClick={() => handleOpenPref(p.key)}
                >
                  {t("onboarding.perms.go_open")}
                </button>
              )}
            </div>
          );
        })}
      </div>

      {/* v0.1.26 · 开机自启动开关 —— 放在权限列表之后，cheatsheet 之前。
          checkbox 直接切系统状态 + config 同步。 */}
      <div className="ob-perm-row" style={{ marginTop: 12 }}>
        <span className="ob-perm-icon">🚀</span>
        <div className="ob-perm-text">
          <span className="ob-perm-title">{t("onboarding.autostart.title")}</span>
          <span className="ob-perm-desc">{t("onboarding.autostart.sub")}</span>
        </div>
        <label style={{ display: "inline-flex", alignItems: "center", cursor: "pointer" }}>
          <input
            type="checkbox"
            checked={autostart}
            onChange={async (e) => {
              const want = e.target.checked;
              setAutostart(want);
              try {
                const actual = await invoke<boolean>("set_autostart", { enable: want });
                if (actual !== want) setAutostart(actual);
              } catch {
                setAutostart(!want);
              }
            }}
            style={{ width: 22, height: 22, cursor: "pointer" }}
          />
        </label>
      </div>

      {/* 主按钮永远可点 —— 即使检测有偏差也不卡死用户。
          allDone 时是「完成并重启」主样式；否则是「跳过检查」次要样式。 */}
      <button
        type="button"
        className={`ob-cta ${!allDone ? "ob-cta-secondary" : ""}`}
        onClick={() => onComplete(selected, backend, skin, voiceImeTrigger, petAnchor, voiceLang)}
      >
        {allDone ? t("onboarding.cta.finish") : t("onboarding.cta.skip")}
      </button>

      <p className="ob-skip-hint">
        {allDone ? t("onboarding.skip_hint.all_done") : t("onboarding.skip_hint.partial")}
      </p>

      {/* v0.1.17 快捷键 cheat sheet —— 解决 ⌘⇧V 发现性问题 */}
      <div className="ob-cheatsheet">
        <div className="ob-cheatsheet-title">{t("onboarding.cheatsheet.title")}</div>
        <table>
          <tbody>
            <tr>
              <td><kbd>⌘</kbd><kbd>⇧</kbd><kbd>Space</kbd></td>
              <td>{t("onboarding.cheatsheet.summon")}</td>
            </tr>
            <tr>
              <td><kbd>⌘</kbd><kbd>⇧</kbd><kbd>V</kbd></td>
              <td>{t("onboarding.cheatsheet.clipboard")}</td>
            </tr>
            <tr>
              <td><kbd>{voiceImeTrigger === "disabled" ? "—" :
                voiceImeTrigger === "fn" ? "fn"
                : voiceImeTrigger === "option" ? "⌥"
                : voiceImeTrigger === "control" ? "⌃"
                : voiceImeTrigger === "right-shift" ? "right ⇧"
                : voiceImeTrigger === "right-command" ? "right ⌘"
                : "right ⌥"}</kbd></td>
              <td>{t("onboarding.cheatsheet.voice_ime")}</td>
            </tr>
            <tr>
              <td>🖱️</td>
              <td>{t("onboarding.cheatsheet.click_pet")}</td>
            </tr>
          </tbody>
        </table>
        <p className="ob-cheatsheet-foot">{t("onboarding.cheatsheet.tray_hint")}</p>
      </div>
    </div>
  );
}

/**
 * 桌宠 anchor 预览 —— 一个 mini desktop（mock menubar + dock），
 * 老鼠按 pos 落在对应角落或居中（follow）。视觉与 prototype 对齐：
 * docs/prototypes/pet-anchor-menu-nudges-20260518.html §1
 */
function AnchorPreview(
  { pos, skin }: { pos: "tl" | "tr" | "bl" | "br" | "follow" | "hidden"; skin: SkinId },
) {
  const petStyle: CSSProperties = (() => {
    switch (pos) {
      case "tl":     return { top: 10, left: 8 };
      case "tr":     return { top: 10, right: 8 };
      case "bl":     return { bottom: 14, left: 8 };
      case "br":     return { bottom: 14, right: 8 };
      case "follow": return { top: "50%", left: "50%", transform: "translate(-50%, -50%)" };
      case "hidden": return { display: "none" };
    }
  })();
  return (
    <div
      aria-hidden
      style={{
        position: "relative",
        width: "100%",
        height: 76,
        background: "linear-gradient(180deg, #2c2e44 0%, #1e2034 100%)",
        borderRadius: 8,
        overflow: "hidden",
      }}
    >
      {/* mock menubar */}
      <div style={{
        position: "absolute", top: 0, left: 0, right: 0, height: 6,
        background: "rgba(0,0,0,0.4)",
      }} />
      {/* mock dock */}
      <div style={{
        position: "absolute", bottom: 3, left: "50%", transform: "translateX(-50%)",
        width: 40, height: 6, background: "rgba(255,255,255,0.08)", borderRadius: 3,
      }} />
      {/* pet (or follow indicator) */}
      <div style={{ position: "absolute", ...petStyle }}>
        <PixelMouse state="sleep" size={32} skin={skin} />
      </div>
      {pos === "hidden" && (
        <span style={{
          position: "absolute",
          inset: 0,
          display: "grid",
          placeItems: "center",
          fontSize: 22,
          opacity: 0.6,
        }}>👻</span>
      )}
      {pos === "follow" && (
        <span
          style={{
            position: "absolute",
            top: "50%",
            left: "calc(50% - 18px)",
            transform: "translateY(-50%)",
            width: 7, height: 7, borderRadius: "50%",
            background: "#fff",
            boxShadow: "0 0 6px rgba(255,255,255,0.6)",
          }}
        />
      )}
    </div>
  );
}
