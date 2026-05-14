/**
 * Two-step Onboarding:
 *   Step 1 — pick a global shortcut
 *   Step 2 — grant required permissions (Accessibility, Screen Recording, Microphone)
 *
 * Per DESIGN.md §4.5.
 */
import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { PixelMouse } from "./PixelMouse";
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
  onComplete: (choice: ShortcutChoice) => void;
}

const OPTIONS: Array<{
  id: ShortcutChoice; label: string; keyHint: string; tag?: string;
}> = [
  { id: "hold-option", label: "按住 ⌃ + ⌘ + 空格", keyHint: "⌃ ⌘ Space", tag: "零冲突 · 推荐" },
  { id: "hold-cmd",    label: "按住 ⌃ + ⌘ + M",    keyHint: "⌃ ⌘ M" },
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
  const [step, setStep] = useState<1 | 2>(1);
  const [selected, setSelected] = useState<ShortcutChoice>("hold-option");
  const [perms, setPerms] = useState<PermissionStatus>({
    accessibility: false,
    screen_recording: false,
    microphone: false,
  });

  // 轮询权限状态（用户去系统设置授权后自动刷新）
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
    if (step !== 2) return;
    refreshPerms();
    // 每 2 秒刷新一次，让用户授权后立即看到变化
    const timer = setInterval(refreshPerms, 2000);
    return () => clearInterval(timer);
  }, [step, refreshPerms]);

  const allGranted =
    perms.accessibility && perms.screen_recording && perms.microphone;

  const handleOpenPref = async (key: keyof PermissionStatus) => {
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
          <PixelMouse state="listen" size={96} />
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
          下一步：开启权限 →
        </button>
      </div>
    );
  }

  // ── Step 2: 权限申请 ──────────────────────────────────────────────────────
  return (
    <div className="ob-root">
      <div className="ob-mouse-stage">
        <PixelMouse state={allGranted ? "jump" : "think"} size={96} />
      </div>
      <h1 className="ob-title">开启必要权限</h1>
      <p className="ob-subtitle">
        点击每一项，在弹出的系统设置里勾选 MouseClaw。<br />
        授权后这里会自动变绿 ✓
      </p>

      <div className="ob-perm-list">
        {PERMISSIONS.map(p => {
          const granted = perms[p.key];
          return (
            <div
              key={p.key}
              className={`ob-perm-row ${granted ? "granted" : ""}`}
            >
              <span className="ob-perm-icon">{p.icon}</span>
              <div className="ob-perm-text">
                <span className="ob-perm-title">{p.title}</span>
                <span className="ob-perm-desc">{p.desc}</span>
              </div>
              {granted ? (
                <span className="ob-perm-check" aria-label="已授权">✓</span>
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

      <button
        type="button"
        className={`ob-cta ${!allGranted ? "ob-cta-secondary" : ""}`}
        onClick={() => onComplete(selected)}
        aria-disabled={!allGranted}
      >
        {allGranted ? "开始使用 🦞" : "跳过（部分功能不可用）"}
      </button>

      {!allGranted && (
        <p className="ob-skip-hint">
          建议全部开启后再使用，否则快捷键或录音可能无法工作。
        </p>
      )}
    </div>
  );
}
