/**
 * v0.4.0 首次使用引导气泡（overlay tour，5 步）：
 *   1 欢迎 · 2 准备网页 · 3 教 ⌘⇧Space · 4 教 fn · 5 庆祝
 * 每步用户可按按钮 advance 或退出引导。
 *
 * ⚠️ helper（ChipHeader / Btn / kbd / TOUR_WRAP_STYLE）**必须**在组件外（模块级）。
 * 若定义在组件函数体内，App 频繁重渲染（桌宠眼球追鼠标 / companion 状态随鼠标移动
 * 更新）时它们每次都是新组件类型 → React 把按钮卸载重建 → 用户移鼠标去点的瞬间按钮被
 * remount，mousedown/mouseup 落在不同(被替换)的元素上 → onClick 永不触发
 * （2026-05-21 用户报"tour 按钮点不动"的根因）。提到模块级后引用稳定，按钮不再 remount。
 * 回归测试见 TourBubble.test.tsx。
 */
import { type CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";

const TOUR_WRAP_STYLE: CSSProperties = {
  background: "#fff", border: "2px solid #e8d8dc",
  borderRadius: 22, padding: "16px 22px",
  fontSize: 14, maxWidth: 360, lineHeight: 1.5,
  boxShadow: "0 8px 32px rgba(43,38,34,.18)",
  pointerEvents: "auto",
  // 防止"想点按钮却选中了文字"——整个气泡禁选（按钮本就不可选）
  userSelect: "none", WebkitUserSelect: "none",
  fontFamily: "-apple-system, 'PingFang SC', system-ui, sans-serif",
  color: "#2b2622",
};

function ChipHeader({ children }: { children: React.ReactNode }) {
  return <div style={{
    display: "inline-block",
    background: "#f3eee5", color: "#8a8178",
    fontSize: 11, padding: "2px 10px", borderRadius: 999,
    marginBottom: 8, letterSpacing: "0.03em",
  }}>{children}</div>;
}

function Btn({ primary, onClick, children }: { primary?: boolean; onClick: () => void; children: React.ReactNode }) {
  return <button
    type="button"
    onClick={onClick}
    style={{
      font: "inherit", fontSize: 13, padding: "6px 14px",
      borderRadius: 999, cursor: "pointer",
      border: primary ? 0 : "1px solid #ece6dd",
      background: primary ? "#e8638c" : "#f0eae0",
      color: primary ? "#fff" : "#2b2622",
      fontWeight: primary ? 600 : 500,
    }}
  >{children}</button>;
}

function kbd(k: string) {
  return <kbd style={{
    background: "#f0eae0", border: "1px solid #ddd", borderBottomWidth: 2,
    padding: "1px 6px", borderRadius: 4,
    fontFamily: "ui-monospace, Menlo, monospace", fontSize: 12,
  }}>{k}</kbd>;
}

export function TourBubble({ step }: { step: number }) {
  const advance = (next: number) => invoke("tour_advance", { step: next }).catch(() => {});
  const skip = () => invoke("tour_skip").catch(() => {});
  const wrapStyle = TOUR_WRAP_STYLE;

  if (step === 1) {
    return (
      <div data-adaptive-measure="" style={wrapStyle}>
        <ChipHeader>🎓 第 1 步 / 共 5 步</ChipHeader>
        <strong style={{ color: "#d63d6a" }}>下载完啦！</strong> 我能听见你说话，<br />
        帮你召唤本地 AI、做语音输入。<br />
        <span style={{ fontSize: 13, color: "#8a8178", display: "block", marginTop: 6 }}>
          花 60 秒教你怎么用？
        </span>
        <div style={{ marginTop: 12, display: "flex", gap: 10 }}>
          <Btn primary onClick={() => advance(2)}>✨ 好啊</Btn>
          <Btn onClick={skip}>下次再说</Btn>
        </div>
      </div>
    );
  }
  if (step === 2) {
    return (
      <div data-adaptive-measure="" style={wrapStyle}>
        <ChipHeader>🎓 第 2 步 / 5 · 准备目标</ChipHeader>
        随便<strong style={{ color: "#d63d6a" }}>打开一个网页</strong>，比如<br />
        公众号文章 / 维基 / 新闻。<br />
        <span style={{ fontSize: 13, color: "#8a8178", display: "block", marginTop: 6 }}>
          打开后回来这里继续 ↓
        </span>
        <div style={{ marginTop: 12, display: "flex", gap: 10 }}>
          <Btn primary onClick={() => advance(3)}>✓ 已经打开了</Btn>
          <Btn onClick={skip}>退出引导</Btn>
        </div>
      </div>
    );
  }
  if (step === 3) {
    return (
      <div data-adaptive-measure="" style={wrapStyle}>
        <ChipHeader>🎓 第 3 步 / 5 · 召唤 AI</ChipHeader>
        按住 {kbd("⌘")} {kbd("⇧")} {kbd("Space")} 然后说：<br />
        <em style={{
          background: "#e6f4ec", padding: "4px 10px", borderRadius: 6,
          display: "inline-block", margin: "6px 0",
        }}>"用三句话总结这个页面"</em><br />
        <span style={{ fontSize: 12, color: "#8a8178", display: "block", marginTop: 4 }}>
          松开快捷键 = 我开始干活
        </span>
        <div style={{ marginTop: 12, display: "flex", gap: 10 }}>
          <Btn primary onClick={() => advance(4)}>✓ 我学会了 → 下一步</Btn>
          <Btn onClick={skip}>退出</Btn>
        </div>
      </div>
    );
  }
  if (step === 4) {
    return (
      <div data-adaptive-measure="" style={wrapStyle}>
        <ChipHeader>🎓 第 4 步 / 5 · 语音打字</ChipHeader>
        再教你<strong style={{ color: "#d63d6a" }}>一招</strong> —— 任何输入框里<br />
        长按 {kbd("fn")} 说话，字会打到光标位置。<br />
        <span style={{ fontSize: 12, color: "#8a8178", display: "block", marginTop: 6 }}>
          试试在 Spotlight ({kbd("⌘")}Space) 或微信里<br />
          长按 fn 说"今天天气真好"
        </span>
        <div style={{ marginTop: 12, display: "flex", gap: 10 }}>
          <Btn primary onClick={() => advance(5)}>✓ 学会了 → 完成</Btn>
          <Btn onClick={skip}>退出</Btn>
        </div>
      </div>
    );
  }
  // step >= 5 庆祝
  return (
    <div data-adaptive-measure="" style={{ ...wrapStyle, background: "#e6f4ec", borderColor: "#b8e8c3" }}>
      <strong style={{ color: "#1a6b3a", fontSize: 16 }}>🎉 你学会了！</strong><br />
      以后任何时候按 {kbd("⌘")} {kbd("⇧")} {kbd("Space")} 就能召唤我。<br />
      <span style={{ fontSize: 12, color: "#5a5249", marginTop: 8, display: "block" }}>
        🍽 拖文件给我 = 喂我读 · 🦞 点我头开菜单
      </span>
      <span style={{ fontSize: 11, color: "#7a7167", marginTop: 6, display: "block" }}>
        （5 秒后自动消失 · 点桌宠菜单「📖 教我用」可重看）
      </span>
    </div>
  );
}
