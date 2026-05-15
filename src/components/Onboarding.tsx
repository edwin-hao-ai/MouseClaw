/**
 * Four-step Onboarding:
 *   Step 1 — pick a global shortcut
 *   Step 2 — pick an AI backend (Claude Code / Codex / OpenClaw CLI)
 *   Step 3 — pick a desktop pet skin (6 mouse styles)
 *   Step 4 — grant required permissions (Accessibility, Screen Recording, Microphone)
 *
 * Per DESIGN.md §4.5.
 */
import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { PixelMouse } from "./PixelMouse";
import type { BackendChoice, SkinId } from "../types";
import { SKINS, DEFAULT_SKIN } from "../skins";
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

interface OnboardingProps {
  onComplete: (choice: ShortcutChoice, backend: BackendChoice, skin: SkinId) => void;
}

const OPTIONS: Array<{
  id: ShortcutChoice; label: string; keyHint: string; tag?: string;
}> = [
  { id: "hold-option", label: "按住 ⌃ + ⌘ + 空格", keyHint: "⌃ ⌘ Space", tag: "零冲突 · 推荐" },
  { id: "hold-cmd",    label: "按住 ⌃ + ⌘ + M",    keyHint: "⌃ ⌘ M" },
];

const BACKENDS: Array<{
  id: BackendChoice; label: string; desc: string; tag?: string;
}> = [
  {
    id: "claude-cli",
    label: "Claude Code CLI",
    desc: "最成熟 · 原生 agentic + 读图。需已装并登录 claude。",
    tag: "推荐",
  },
  {
    id: "codex-cli",
    label: "OpenAI Codex CLI",
    desc: "codex exec 非交互模式。需 npm i -g @openai/codex 并配好 key。",
  },
  {
    id: "openclaw-cli",
    label: "OpenClaw CLI",
    desc: "openclaw agent --local。需 npm i -g openclaw 并配好 provider key。",
  },
];

const PERMISSIONS: Array<{
  key: keyof PermissionStatus;
  icon: string;
  title: string;
  desc: string;
}> = [
  {
    key: "accessibility",
    icon: "⌨️",
    title: "辅助功能",
    desc: "全局快捷键必须。前往「系统设置 → 隐私与安全性 → 辅助功能」开启。",
  },
  {
    key: "screen_recording",
    icon: "🖥️",
    title: "屏幕录制",
    desc: "截图给 AI 看必须。前往「系统设置 → 隐私与安全性 → 屏幕录制」开启。",
  },
  {
    key: "microphone",
    icon: "🎙️",
    title: "麦克风",
    desc: "语音输入必须。前往「系统设置 → 隐私与安全性 → 麦克风」开启。",
  },
];

export function Onboarding({ onComplete }: OnboardingProps) {
  const [step, setStep] = useState<1 | 2 | 3 | 4>(1);
  const [selected, setSelected] = useState<ShortcutChoice>("hold-option");
  const [backend, setBackend] = useState<BackendChoice>("claude-cli");
  const [skin, setSkin] = useState<SkinId>(DEFAULT_SKIN);
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
    if (step !== 4) return;
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
        <h1 className="ob-title">嘿，我是鼠标龙虾 🦞</h1>
        <p className="ob-subtitle">
          <strong>按住</strong> 快捷键说话，<strong>松开</strong> 发给 AI。<br />
          选一个不和别的应用冲突的键。
        </p>
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
          下一步：选 AI 后端 →
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
        <h1 className="ob-title">选一个 AI 后端</h1>
        <p className="ob-subtitle">
          MouseClaw 把语音 + 截图交给它处理。<br />
          不确定就选 <strong>Claude Code CLI</strong>（最成熟）。
        </p>
        <div className="ob-options" role="radiogroup" aria-label="选择 AI 后端">
          {BACKENDS.map(b => (
            <button
              key={b.id}
              type="button"
              role="radio"
              aria-checked={backend === b.id}
              className={`ob-option ${backend === b.id ? "selected" : ""}`}
              onClick={() => setBackend(b.id)}
            >
              <span className="ob-label">{b.label}</span>
              <span className="ob-perm-desc">{b.desc}</span>
              {b.tag && <span className="ob-tag">{b.tag}</span>}
            </button>
          ))}
        </div>
        <button
          type="button"
          className="ob-cta"
          onClick={() => setStep(3)}
        >
          下一步：挑桌宠 →
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
        <h1 className="ob-title">挑一只你的桌宠 🦞</h1>
        <p className="ob-subtitle">
          6 款老鼠风格 · 随时可在<strong>托盘菜单「换个桌宠」</strong>切换。
        </p>
        <div
          className="ob-options"
          role="radiogroup"
          aria-label="选择桌宠皮肤"
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
          下一步：开启权限 →
        </button>
      </div>
    );
  }

  // ── Step 4: 权限申请 ──────────────────────────────────────────────────────
  return (
    <div className="ob-root">
      <div className="ob-mouse-stage">
        <PixelMouse state={allDone ? "jump" : "think"} size={96} skin={skin} />
      </div>
      <h1 className="ob-title">开启必要权限</h1>
      <p className="ob-subtitle">
        点「去开启」会弹出系统授权框，按提示勾选 MouseClaw。<br />
        辅助功能 / 麦克风授权后会自动变绿；<strong>屏幕录制需要重启 App 才生效</strong>。
      </p>

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
                <span className="ob-perm-check" aria-label="已授权">✓</span>
              ) : pendingRestart ? (
                <span className="ob-perm-pending" aria-label="已请求，重启生效">
                  已请求 · 重启生效
                </span>
              ) : (
                <button
                  type="button"
                  className="ob-perm-btn"
                  onClick={() => handleOpenPref(p.key)}
                >
                  去开启
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
        onClick={() => onComplete(selected, backend, skin)}
      >
        {allDone ? "完成并重启 MouseClaw 🦞" : "已在系统设置里开好了 → 完成并重启"}
      </button>

      <p className="ob-skip-hint">
        {allDone
          ? "点击后会重启 App —— 这是让屏幕录制权限生效的必要步骤。"
          : "如果你已经在「系统设置 → 隐私与安全性」里手动勾选了 MouseClaw，可以直接点上面完成。重启后会重新检测。"}
      </p>
    </div>
  );
}
