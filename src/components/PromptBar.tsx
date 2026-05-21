/**
 * Listening-state input — temporary stand-in for voice capture.
 * Same visual envelope as Bubble (white pill, pointer-down), but with a
 * text input + submit button instead of static body text.
 *
 * Day 3+ replaces this with a real microphone widget driven by cpal + sherpa-onnx.
 */
import { useState } from "react";
import "./Bubble.css";
import "./PromptBar.css";

interface PromptBarProps {
  onSubmit: (text: string) => void;
  /** Cancel callback (Esc). */
  onCancel: () => void;
  placeholder?: string;
}

export function PromptBar({ onSubmit, onCancel, placeholder = "想问什么？(Enter 发送 / Esc 取消)" }: PromptBarProps) {
  const [text, setText] = useState("");

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    const t = text.trim();
    if (!t) return;
    onSubmit(t);
    setText("");
  };

  return (
    <form className="bubble bubble-default prompt-bar" onSubmit={submit}>
      <input
        type="text"
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => { if (e.key === "Escape") onCancel(); }}
        placeholder={placeholder}
        autoFocus
        aria-label="输入你的问题"
      />
      <button type="submit" className="prompt-bar-send" aria-label="发送">↑</button>
    </form>
  );
}
