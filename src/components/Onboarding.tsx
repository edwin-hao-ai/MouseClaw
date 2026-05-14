/**
 * One-step Onboarding — pick a global shortcut, then disappear forever.
 * Per DESIGN.md §4.5.
 */
import { useState } from "react";
import { PixelMouse } from "./PixelMouse";
import "./Onboarding.css";

export type ShortcutChoice =
  | "double-option"
  | "hold-option"
  | "double-cmd"
  | "hold-cmd";

interface OnboardingProps {
  onComplete: (choice: ShortcutChoice) => void;
}

const OPTIONS: Array<{
  id: ShortcutChoice; label: string; keyHint: string; tag?: string;
}> = [
  { id: "hold-option", label: "按住 ⌃ + ⌘ + 空格", keyHint: "⌃ ⌘ Space", tag: "零冲突 · 推荐" },
  { id: "hold-cmd",    label: "按住 ⌃ + ⌘ + M",    keyHint: "⌃ ⌘ M" },
];

export function Onboarding({ onComplete }: OnboardingProps) {
  const [selected, setSelected] = useState<ShortcutChoice>("hold-option");

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
        onClick={() => onComplete(selected)}
      >
        开始使用 →
      </button>
    </div>
  );
}
