/**
 * Onboarding window (independent Tauri window opened from Rust on first launch).
 * Replaces the v0.1.3 approach of rendering Onboarding inside the 320×320 overlay
 * (which was too cramped). This window is 700×760, has a real titlebar, and gets
 * its own URL route `?view=onboarding`.
 */
import { invoke } from "@tauri-apps/api/core";
import { Onboarding } from "./components/Onboarding";
import type { ShortcutChoice, BackendChoice } from "./types";

export default function OnboardingView() {
  const handleComplete = async (choice: ShortcutChoice, backend: BackendChoice) => {
    try {
      // 1. 持久化快捷键 + AI 后端选择 + 标记 onboarded=true
      await invoke("save_shortcut", { choice, backend });
      // 2. 重启 App —— 屏幕录制权限授权后必须重启本进程才生效（macOS 设计）。
      //    重启后 onboarded=true → 直接注册快捷键 + ready，屏幕录制也活了。
      await invoke("restart_app");
    } catch (e) {
      console.error("onboarding complete failed:", e);
      alert("完成 Onboarding 失败：" + String(e));
    }
  };

  return (
    <div className="ob-view-host">
      <Onboarding onComplete={handleComplete} />
    </div>
  );
}
