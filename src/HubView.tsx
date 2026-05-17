/**
 * HubView · 剪贴板 + AI 历史 独立窗口 (v0.1.16)
 *
 * v0.1.15 之前 Hub 钉在 320×320 transparent overlay 里，跟桌宠抢空间 + 桌宠还跟着
 * 鼠标动 → 用户没法选项。改成独立窗口（同 Panel 方案），用户聚焦自然，
 * 不被桌宠遮挡。
 */
import { useCallback, useEffect } from "react";
import { Hub } from "./components/Hub";

export default function HubView() {
  const close = useCallback(() => {
    // 关掉本窗口 —— Tauri webview hide()，下次打开时 show()
    import("@tauri-apps/api/webviewWindow").then(({ getCurrentWebviewWindow }) => {
      const w = getCurrentWebviewWindow();
      w.hide().catch(() => {});
    });
  }, []);

  // 任何 invoke "paste_clipboard_item" 都需要 Hub 先消失才能让原 app 重新成 frontmost
  // Hub 内部 handlePaste 已经先 onClose → setTimeout 100ms → paste → 现在 onClose=close
  // close() 会 hide 窗口，焦点回到上一 app

  // Esc 全局关
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        close();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [close]);

  // 焦点丢失（用户点窗外）→ 自动关
  useEffect(() => {
    const onBlur = () => close();
    window.addEventListener("blur", onBlur);
    return () => window.removeEventListener("blur", onBlur);
  }, [close]);

  // 自适应窗口高度：内容很高时窗口跟着撑，不要内部双滚动
  // 留给 Hub 组件自己的滚动管，这里就是个 host

  return (
    <div style={{
      // v0.1.18：Hub 直接铺满整个独立窗口（窗口本身 420×480）
      // 之前外层加 24px paddingTop + maxWidth 540 浪费空间又不一致
      width: "100vw", height: "100vh",
      background: "#faf7f2",
      fontFamily: "-apple-system, 'PingFang SC', system-ui, sans-serif",
      overflow: "hidden",
    }}>
      <Hub onClose={close} />
    </div>
  );
}
