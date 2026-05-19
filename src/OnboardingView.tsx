/**
 * Onboarding window (independent Tauri window opened from Rust on first launch).
 * Replaces the v0.1.3 approach of rendering Onboarding inside the 320×320 overlay
 * (which was too cramped). This window is 700×760, has a real titlebar, and gets
 * its own URL route `?view=onboarding`.
 */
import { invoke } from "@tauri-apps/api/core";
import { Onboarding, type VoiceImeTrigger, type VoiceLang } from "./components/Onboarding";
import type { ShortcutChoice, BackendChoice, SkinId, PetAnchor } from "./types";

export default function OnboardingView() {
  const handleComplete = async (
    choice: ShortcutChoice, backend: BackendChoice, skin: SkinId,
    voiceImeTrigger: VoiceImeTrigger,
    petAnchor: PetAnchor,
    voiceLang: VoiceLang,
  ) => {
    try {
      // 1. 持久化主快捷键 + AI 后端 + 皮肤 + 语音模型语言
      // v0.4.0 · voiceLang 决定下哪个 sherpa 模型；save_shortcut 返回后会立即触发后台下载
      await invoke("save_shortcut", { choice, backend, skin, voiceLang });
      // 2. 持久化 voice IME 触发键 + enable 状态
      if (voiceImeTrigger === "disabled") {
        await invoke("save_voice_ime", { enabled: false });
      } else {
        await invoke("save_voice_ime", { enabled: true });
        await invoke("save_voice_ime_trigger", { trigger: voiceImeTrigger });
      }
      // 3. v0.1.27 · 持久化桌宠悬停位置
      await invoke("save_pet_anchor", { anchor: petAnchor });
      // 4. 重启 App —— 屏幕录制权限授权后必须重启本进程才生效（macOS 设计）。
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
