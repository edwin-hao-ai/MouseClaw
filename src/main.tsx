import React from "react";
import ReactDOM from "react-dom/client";
import "./styles/tokens.css";
import { shouldInstallDevTauriMock, installDevTauriMock } from "./lib/dev-tauri-mock";

// v0.4 E2E · 普通 Chromium / Chrome MCP 跑组件时装上 Tauri 桩。
// 触发：localStorage.setItem("mouseclaw.e2e.tauriMock", "1") + reload。
if (shouldInstallDevTauriMock()) {
  installDevTauriMock();
}

// Route based on ?view= query param so a single bundle serves four windows:
//   ?view=history    → HistoryView
//   ?view=about      → AboutView
//   ?view=onboarding → OnboardingView
//   (default)        → App (the mouse overlay)
const params = new URLSearchParams(window.location.search);
const view = params.get("view");

/**
 * tokens.css forces `html, body, #root { height:100%; overflow:hidden }` —
 * correct for the transparent fixed-size overlay window, but it CLIPS the
 * scrollable content of the regular windows (history/about/onboarding).
 * For those, switch html/body/#root to a normal scrolling document.
 */
function makeScrollableDocument(bg: string) {
  for (const el of [document.documentElement, document.body]) {
    el.style.background = bg;
    el.style.height = "auto";
    el.style.minHeight = "100%";
    el.style.overflow = "auto";
  }
  const root = document.getElementById("root");
  if (root) {
    root.style.height = "auto";
    root.style.minHeight = "100vh";
    root.style.overflow = "visible";
  }
}

async function bootstrap() {
  let RootComp: React.ComponentType;
  if (view === "history") {
    makeScrollableDocument("#f5f5f7");
    const { default: HistoryView } = await import("./HistoryView");
    RootComp = HistoryView;
  } else if (view === "about") {
    makeScrollableDocument("#f5f5f7");
    const { default: AboutView } = await import("./AboutView");
    RootComp = AboutView;
  } else if (view === "status") {
    makeScrollableDocument("#f5f5f7");
    const { default: StatusView } = await import("./StatusView");
    RootComp = StatusView;
  } else if (view === "panel") {
    makeScrollableDocument("#faf7f2");
    const { default: PanelView } = await import("./PanelView");
    RootComp = PanelView;
  } else if (view === "hub") {
    makeScrollableDocument("#faf7f2");
    const { default: HubView } = await import("./HubView");
    RootComp = HubView;
  } else if (view === "picker") {
    makeScrollableDocument("#faf7f2");
    const { default: PickerView } = await import("./PickerView");
    RootComp = PickerView;
  } else if (view === "tasks") {
    makeScrollableDocument("#f5f5f7");
    const { default: TasksView } = await import("./TasksView");
    RootComp = TasksView;
  } else if (view === "draw") {
    // 全屏透明 overlay，不能用 scrollable document
    document.documentElement.style.background = "transparent";
    document.body.style.background = "transparent";
    const root = document.getElementById("root");
    if (root) root.style.background = "transparent";
    const { default: DrawOverlay } = await import("./DrawOverlay");
    RootComp = DrawOverlay;
  } else if (view === "downloader") {
    // v0.4.0 · 模型下载进度独立窗口 —— 监听 model-progress 事件展示总进度
    makeScrollableDocument("#faf7f2");
    const { default: DownloaderView } = await import("./DownloaderView");
    RootComp = DownloaderView;
  } else if (view === "onboarding") {
    // Onboarding card brings its own dark glass background; host wraps it
    // in a centered flex container with a faint matching gradient backdrop.
    makeScrollableDocument("#0a0e1a");
    const { default: OnboardingView } = await import("./OnboardingView");
    RootComp = OnboardingView;
  } else {
    await import("./App.css");
    const { default: App } = await import("./App");
    RootComp = App;
  }

  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <RootComp />
    </React.StrictMode>,
  );
}

bootstrap();
