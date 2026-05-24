/**
 * Text-input bubble — v0.5.x · 召唤后不想语音说话时的文字输入态。
 *
 * 触发：listening 态用户敲任意字符键 → App.tsx invoke `switch_to_text_input(initial)`
 *   → Rust 停录音丢音频 + emit `text-input` 视图 → 这个气泡渲染。
 * `initial` = 触发切换的那个字符（已敲的字不丢，塞进输入框）。
 * 提交走 `submit_query` → 主 pipeline，和语音转写完全同一条路（截图复用召唤瞬间那张）。
 *
 * overlay 平时是非激活 NSPanel（focus:false）—— 能打字靠 App.tsx 在进入此视图时
 * invoke `set_overlay_focusable(true)`（离开时关掉），参考 voice-confirm 编辑路径。
 */
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useT } from "../i18n";
import "./Bubble.css";
import "./TextInputBubble.css";

interface TextInputBubbleProps {
  initial?: string;
}

export function TextInputBubble({ initial = "" }: TextInputBubbleProps) {
  const t = useT();
  const [text, setText] = useState(initial);
  const inputRef = useRef<HTMLInputElement>(null);

  // 进来即 focus，光标落在已敲字符之后（无缝接着打）。
  useEffect(() => {
    const el = inputRef.current;
    if (!el) return;
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
  }, []);

  const submit = () => {
    const trimmed = text.trim();
    if (trimmed.length === 0) {
      // 空内容提交 = 没必要发空 prompt 给 AI → 当作取消召唤。
      invoke("dismiss").catch(() => {});
      return;
    }
    invoke("submit_query", { text: trimmed }).catch((e) =>
      console.warn("submit_query failed:", e));
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    // 阻止冒泡到 App 全局 keydown（此时 view 已非 listening，稳妥起见仍拦）。
    e.stopPropagation();
    if (e.key === "Enter") {
      e.preventDefault();
      submit();
    } else if (e.key === "Escape") {
      e.preventDefault();
      invoke("dismiss").catch(() => {});
    }
  };

  return (
    // data-adaptive-measure —— useAdaptiveOverlay 据此测量，输入框比 listening 气泡高，
    // 窗口自动撑大（别去拍 EXPANDED_SIZE）。
    <div className="bubble bubble-default text-input-bubble" data-adaptive-measure="">
      <div className="ti-row">
        <input
          ref={inputRef}
          className="ti-input"
          value={text}
          placeholder={t("textinput.placeholder")}
          aria-label={t("textinput.placeholder")}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={onKeyDown}
        />
        <button
          type="button"
          className="ti-send"
          onClick={submit}
          aria-label={t("textinput.send")}
          title={t("textinput.send")}
        >
          ↑
        </button>
      </div>
      <div className="ti-foot">
        <span><kbd>↵</kbd> {t("textinput.send")}</span>
        <span><kbd>Esc</kbd> {t("textinput.cancel")}</span>
      </div>
    </div>
  );
}
