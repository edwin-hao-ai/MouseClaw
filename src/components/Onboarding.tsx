/**
 * Five-step Onboarding (v0.1.12):
 *   Step 1 — pick a global shortcut (AI summon)
 *   Step 2 — pick an AI backend
 *   Step 3 — pick a desktop pet skin
 *   Step 4 — pick voice IME trigger key (NEW)
 *   Step 5 — grant required permissions
 */
import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { PixelMouse } from "./PixelMouse";
import type { BackendChoice, SkinId } from "../types";
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

interface OnboardingProps {
  onComplete: (
    choice: ShortcutChoice,
    backend: BackendChoice,
    skin: SkinId,
    voiceImeTrigger: VoiceImeTrigger,
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
  const [step, setStep] = useState<1 | 2 | 3 | 4 | 5>(1);
  const [selected, setSelected] = useState<ShortcutChoice>("hold-option");
  const [backend, setBackend] = useState<BackendChoice>("claude-cli");
  const [skin, setSkin] = useState<SkinId>(DEFAULT_SKIN);
  const [voiceImeTrigger, setVoiceImeTrigger] = useState<VoiceImeTrigger>("fn");
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
    if (step !== 5) return;
    refreshPerms();
    // 每 1.5 秒刷新一次，让用户授权后立即看到变化
    const timer = setInterval(refreshPerms, 1500);
    return () => clearInterval(timer);
  }, [step, refreshPerms]);

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
          {BACKENDS_META.map(b => (
            <button
              key={b.id}
              type="button"
              role="radio"
              aria-checked={backend === b.id}
              className={`ob-option ${backend === b.id ? "selected" : ""}`}
              onClick={() => setBackend(b.id)}
            >
              <span className="ob-label">{b.label}</span>
              <span className="ob-perm-desc">{backendDesc(b.id, t)}</span>
              {b.tag && <span className="ob-tag">{t(b.tag as "common.recommended")}</span>}
            </button>
          ))}
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

  // ── Step 4: 选 voice IME 触发键 (NEW v0.1.12) ─────────────────────────────
  if (step === 4) {
    const triggers: VoiceImeTrigger[] = [
      "fn", "option", "control", "right-shift", "right-command", "right-option", "disabled"
    ];
    return (
      <div className="ob-root">
        <div className="ob-mouse-stage">
          <PixelMouse state="listen" size={96} skin={skin} />
        </div>
        <h1 className="ob-title">{t("onboarding.voice_ime.title")}</h1>
        <p className="ob-subtitle">{t("onboarding.voice_ime.subtitle")}</p>
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
          {t("onboarding.cta.next_perms")}
        </button>
      </div>
    );
  }

  // ── Step 5: 权限申请 ──────────────────────────────────────────────────────
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

      {/* 主按钮永远可点 —— 即使检测有偏差也不卡死用户。
          allDone 时是「完成并重启」主样式；否则是「跳过检查」次要样式。 */}
      <button
        type="button"
        className={`ob-cta ${!allDone ? "ob-cta-secondary" : ""}`}
        onClick={() => onComplete(selected, backend, skin, voiceImeTrigger)}
      >
        {allDone ? t("onboarding.cta.finish") : t("onboarding.cta.skip")}
      </button>

      <p className="ob-skip-hint">
        {allDone ? t("onboarding.skip_hint.all_done") : t("onboarding.skip_hint.partial")}
      </p>
    </div>
  );
}
