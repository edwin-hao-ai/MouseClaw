import React from "react";
import ReactDOM from "react-dom/client";
import "./styles/tokens.css";

// Route based on ?view= query param so a single bundle serves three windows:
//   ?view=history  → HistoryView
//   ?view=about    → AboutView
//   (default)      → App (the mouse overlay)
const params = new URLSearchParams(window.location.search);
const view = params.get("view");

async function bootstrap() {
  let RootComp: React.ComponentType;
  if (view === "history") {
    // History window is a regular window — undo the transparent body from tokens.css
    document.documentElement.style.background = "#f5f5f7";
    document.body.style.background = "#f5f5f7";
    document.body.style.overflow = "auto";
    const { default: HistoryView } = await import("./HistoryView");
    RootComp = HistoryView;
  } else if (view === "about") {
    document.documentElement.style.background = "#f5f5f7";
    document.body.style.background = "#f5f5f7";
    const { default: AboutView } = await import("./AboutView");
    RootComp = AboutView;
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
