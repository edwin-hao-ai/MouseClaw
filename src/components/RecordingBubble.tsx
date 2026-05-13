/**
 * Listening-state UI when Whisper is active.
 * Replaces PromptBar from Day 2 (which was text-only).
 *
 * Shows a recording indicator + animated waveform. User presses the global
 * shortcut again (or clicks the stop button) to halt recording and trigger
 * transcription + pipeline.
 */
import { invoke } from "@tauri-apps/api/core";
import "./Bubble.css";
import "./RecordingBubble.css";

export function RecordingBubble() {
  const handleStop = () => {
    // Re-fire the same handler as the shortcut press by toggling via
    // the dedicated command. Falls back to no-op if not registered.
    invoke("toggle_recording").catch((e) => {
      console.warn("toggle_recording not yet registered:", e);
    });
  };

  return (
    <div className="bubble bubble-default recording-bubble">
      <span className="rec-dot" aria-hidden />
      <span className="rec-label">录音中…</span>
      <span className="voicebars" aria-hidden>
        <span /><span /><span /><span /><span />
      </span>
      <button
        type="button"
        className="rec-stop"
        onClick={handleStop}
        aria-label="停止录音"
        title="再按 Cmd+Shift+Space 或点这里停止"
      >
        ◼
      </button>
    </div>
  );
}
