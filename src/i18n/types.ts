/**
 * i18n · 类型与接口（v0.1.9）
 *
 * 一句话理念：
 *   - **不引** react-i18next / i18next（20-50KB），项目就一个小桌宠，自己写 200 行够了
 *   - **类型安全**：所有 key 在 TranslationKeys 里枚举，写错 key TS 编译期报错
 *   - **可扩展**：加新语言 = 多写一个 strings 文件 + 加到 LANGUAGES 数组，零侵入
 *   - **支持插值**：`t("greeting", { name: "Edwin" })` → "你好，Edwin"
 *
 * 加新语言示例（比如日语）：
 *   1. 新文件 src/i18n/ja.ts，导出 `const ja: Strings = { ... }`
 *   2. src/i18n/index.ts 的 LANGUAGES 加 "ja"
 *   3. 完事 —— UI 自动多一个选项
 */

/** 所有支持的语言 ID（kebab BCP-47）。新加语言加这里。 */
export type LangId = "zh" | "en";

/** 翻译字符串字典 —— 所有 key 必须在 zh.ts 里写过（zh 是 fallback 源） */
export interface Strings {
  // ── 通用 ──
  "common.ok": string;
  "common.cancel": string;
  "common.confirm": string;
  "common.next": string;
  "common.back": string;
  "common.done": string;
  "common.retry": string;
  "common.close": string;
  "common.search": string;
  "common.loading": string;
  "common.recommended": string;

  // ── 桌宠 / 主气泡 ──
  "bubble.expand_to_panel": string;        // "💬 继续追问"
  "bubble.listening": string;              // "听着呢…"
  "bubble.thinking": string;               // "正在思考"
  "bubble.session_chip": string;           // "🔗 续 Session #{id} · 第 {turn} 轮"
  "bubble.new_session": string;            // "新对话"
  "bubble.error_prefix": string;           // "⛔ "

  // ── Onboarding ──
  "onboarding.welcome.title": string;
  "onboarding.welcome.subtitle": string;
  "onboarding.step.shortcut": string;
  "onboarding.step.backend": string;
  "onboarding.step.skin": string;
  "onboarding.step.permissions": string;
  "onboarding.shortcut.title": string;
  "onboarding.shortcut.hint": string;
  "onboarding.shortcut.zero_conflict": string;
  "onboarding.backend.title": string;
  "onboarding.backend.subtitle": string;
  "onboarding.backend.uncertain_hint": string;
  "onboarding.skin.title": string;
  "onboarding.skin.subtitle": string;
  "onboarding.perms.title": string;
  "onboarding.perms.subtitle": string;
  "onboarding.perms.go_open": string;
  "onboarding.perms.granted": string;
  "onboarding.perms.requested_restart": string;
  "onboarding.cta.next_backend": string;
  "onboarding.cta.next_skin": string;
  "onboarding.cta.next_perms": string;
  "onboarding.cta.finish": string;
  "onboarding.cta.skip": string;
  "onboarding.skip_hint.all_done": string;
  "onboarding.skip_hint.partial": string;
  "perms.accessibility.title": string;
  "perms.accessibility.desc": string;
  "perms.screen.title": string;
  "perms.screen.desc": string;
  "perms.mic.title": string;
  "perms.mic.desc": string;

  // ── 状态窗 (StatusView) ──
  "status.title": string;
  "status.subtitle": string;
  "status.all_good": string;
  "status.partial": string;
  "status.row.claude.title": string;
  "status.row.claude.good": string;
  "status.row.claude.bad": string;
  "status.row.cdp.title": string;
  "status.row.cdp.good": string;
  "status.row.cdp.bad": string;
  "status.row.cdp.action": string;
  "status.row.cdp.enabling": string;
  "status.row.agentbrowser.title": string;
  "status.row.agentbrowser.good": string;
  "status.row.agentbrowser.bad": string;
  "status.row.acc.title": string;
  "status.row.acc.good": string;
  "status.row.acc.bad": string;
  "status.row.scr.title": string;
  "status.row.scr.good": string;
  "status.row.scr.bad": string;
  "status.row.mic.title": string;
  "status.row.mic.good": string;
  "status.row.mic.bad": string;
  "status.action.go": string;
  "status.tip.first_use": string;

  // ── History (AI 历史窗) ──
  "history.title": string;
  "history.empty": string;
  "history.turn_count": string;     // "{count} turns"
  "history.session_label": string;  // "Session #{id}"
  "history.role.user": string;
  "history.role.assistant": string;

  // ── About ──
  "about.title": string;
  "about.tagline": string;

  // ── 通用错误 ──
  "error.generic": string;
  "error.tauri_unavailable": string;

  // ── Hub（点击桌宠面板） ──
  "hub.title": string;
  "hub.tab.clipboard": string;
  "hub.tab.history": string;
  "hub.search.clipboard": string;
  "hub.search.history": string;
  "hub.empty.clipboard": string;
  "hub.empty.history": string;
  "hub.pin": string;
  "hub.unpin": string;
  "hub.clear": string;
  "hub.foot.paste": string;

  // ── 语言切换 ──
  "lang.zh": string;
  "lang.en": string;
  "lang.menu_title": string;
}

/** 翻译值 —— 字符串 OR (args) => string。后者支持插值。 */
export type TranslationValue =
  | string
  | ((args: Record<string, string | number>) => string);

/** Translation key 联合类型 —— Strings 的所有 key */
export type TranslationKey = keyof Strings;
