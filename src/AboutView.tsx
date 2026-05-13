/**
 * About window — opened from the menubar tray.
 * Tiny info card. Window is fixed 420×340.
 */
import "./HistoryView.css";
import "./AboutView.css";

export default function AboutView() {
  return (
    <div className="hv-root about-root">
      <div className="about-card">
        <div className="about-emoji">🦞</div>
        <h1 className="about-title">MouseClaw</h1>
        <div className="about-version">v0.1.4</div>
        <p className="about-desc">
          像素小老鼠的桌面 AI 助手。<br />
          按快捷键召唤，截屏 + 语音 → AI，回答完自动消失。
        </p>
        <div className="about-meta">
          <div>Tauri 2 · React · whisper.cpp · Claude Code CLI</div>
          <div>设计文档：<code>DESIGN.md</code></div>
        </div>
      </div>
    </div>
  );
}
