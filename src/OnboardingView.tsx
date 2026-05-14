/**
 * Onboarding window (independent Tauri window opened from Rust on first launch).
 * Replaces the v0.1.3 approach of rendering Onboarding inside the 320×320 overlay
 * (which was too cramped). This window is 700×760, has a real titlebar, and gets
 * its own URL route `?view=onboarding`.
 */
import { invoke } from "@tauri-apps/api/core";
import { Onboarding } from "./components/Onboarding";
import type { ShortcutChoice } from "./types";

export default function OnboardingView() {
  const handleComplete = async (choice: ShortcutChoice) => {
    try {
      await invoke("save_shortcut", { choice });
      // Rust closes this window itself after persisting + registering the shortcut.
      // If it doesn't (e.g. running in a browser-only test mode), we fall through
      // and the user can just close the window manually.
    } catch (e) {
      console.error("save_shortcut failed:", e);
      alert("保存快捷键失败：" + String(e));
    }
  };

  return (
    <div className="ob-view-host">
      <Onboarding onComplete={handleComplete} />
    </div>
  );
}
