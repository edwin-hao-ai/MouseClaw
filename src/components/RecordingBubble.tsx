/**
 * Listening-state UI while recording.
 *
 * v0.2 · 边说边出字 —— displays streaming partial transcript from sherpa-onnx
 * Zipformer as user speaks. Partial text is rendered in soft gray; final
 * commits when shortcut is released and pipeline advances to Thinking.
 *
 * User stops recording either by:
 *   - releasing the shortcut they held (default flow)
 *   - clicking the ◼ stop button
 *   - clicking the pet itself (v0.1.30)
 */
import { invoke } from "@tauri-apps/api/core";
import { useT } from "../i18n";
import "./Bubble.css";
import "./RecordingBubble.css";

interface RecordingBubbleProps {
  partial?: string;
}

export function RecordingBubble({ partial }: RecordingBubbleProps = {}) {
  const t = useT();
  const handleStop = () => {
    invoke("toggle_recording").catch((e) => {
      console.warn("toggle_recording not yet registered:", e);
    });
  };

  const hasPartial = partial && partial.trim().length > 0;

  return (
    <div className={`bubble bubble-default recording-bubble ${hasPartial ? "has-partial" : ""}`}>
      <div className="rec-main">
        <span className="rec-dot" aria-hidden />
        {hasPartial ? (
          <span className="rec-partial" aria-live="polite">{partial}</span>
        ) : (
          <span className="rec-label">{t("bubble.listening")}</span>
        )}
        <span className="voicebars" aria-hidden>
          <span /><span /><span /><span /><span />
        </span>
        <button
          type="button"
          className="rec-stop"
          onClick={handleStop}
          aria-label={t("textinput.cancel")}
          title="松开快捷键 / 点这里 / 点桌宠 都可停止"
        >
          ◼
        </button>
      </div>
      {/* v0.5.x · 「⌨️ 打字」按钮：listening 不抢键盘焦点，点这里才切文字输入
          （switch_to_text_input → 进 text-input 态再 make_key）。开口出 partial 后淡出。 */}
      {!hasPartial && (
        <button
          type="button"
          className="rec-type-btn"
          onClick={() => invoke("switch_to_text_input", { initial: "" }).catch(() => {})}
        >
          {t("bubble.type_hint")}
        </button>
      )}
    </div>
  );
}
