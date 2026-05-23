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

  // ── 桌宠身份:起名 + 性格 (v0.4.4) ──
  "picker.identity.heading": string;
  "picker.identity.name_label": string;
  "picker.identity.name_placeholder": string;
  "picker.identity.personality_label": string;
  "picker.identity.persona_hint": string;
  "picker.identity.custom_placeholder": string;
  "persona.warm.name": string;       "persona.warm.preview": string;
  "persona.snarky.name": string;     "persona.snarky.preview": string;
  "persona.minimal.name": string;    "persona.minimal.preview": string;
  "persona.companion.name": string;  "persona.companion.preview": string;
  "persona.pro.name": string;        "persona.pro.preview": string;
  "persona.cheerful.name": string;   "persona.cheerful.preview": string;
  "persona.calm.name": string;       "persona.calm.preview": string;
  "persona.curious.name": string;    "persona.curious.preview": string;
  "persona.tsundere.name": string;   "persona.tsundere.preview": string;
  "persona.custom.name": string;     "persona.custom.preview": string;

  // ── 长期记忆查看器 (v0.4.4) ──
  "memory.title": string;
  "memory.tab.profile": string;
  "memory.tab.history": string;
  "memory.tab.graph": string;
  "memory.graph.empty": string;
  "memory.graph.links": string;
  "memory.used_badge": string;
  "memory.pause_label": string;
  "memory.paused_banner": string;
  "memory.profile.intro": string;
  "memory.profile.empty": string;
  "memory.history.search": string;
  "memory.history.empty": string;
  "memory.clear_all": string;
  "memory.clear_confirm": string;
  "memory.local_note": string;
  "memory.delete": string;
  "memory.importance": string;

  // ── 桌宠 / 主气泡 ──
  "bubble.expand_to_panel": string;        // "💬 继续追问"
  "bubble.scroll_to_top": string;          // "▲ 回到开头"
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
  "onboarding.step.voice_ime": string;
  "onboarding.voice_ime.title": string;
  "onboarding.voice_ime.subtitle": string;
  "onboarding.voice_ime.tip": string;
  "onboarding.voice_ime.disabled": string;
  // v0.4.0 P0 · 语音模型设置（双语锁死后只剩信息提示）
  "onboarding.voice_setup.title": string;
  "onboarding.voice_setup.subtitle": string;
  "onboarding.voice_setup.mixed_label": string;
  "onboarding.voice_setup.mixed_hint": string;
  "onboarding.autostart.title": string;
  "onboarding.autostart.sub": string;
  // Pet anchor (v0.1.27)
  "onboarding.step.anchor": string;
  "onboarding.anchor.title": string;
  "onboarding.anchor.subtitle": string;
  "onboarding.cta.next_anchor": string;
  "anchor.top-left": string;
  "anchor.top-right": string;
  "anchor.bottom-left": string;
  "anchor.bottom-right": string;
  "anchor.follow": string;
  "anchor.hidden": string;
  "anchor.tray_title": string;
  "anchor.follow_hint": string;
  "anchor.hidden_hint": string;

  // Pet click menu (v0.1.27 P2)
  "petmenu.summon": string;
  "petmenu.summon_hint": string;
  "petmenu.recording_hint": string;
  "petmenu.history": string;
  "petmenu.feed": string;
  "petmenu.nap": string;
  "petmenu.skin": string;
  "petmenu.settings": string;
  "petmenu.feed_ack": string;
  "petmenu.nap_ack": string;

  // Nudge bubble (v0.1.27 P3)
  "nudge.later": string;
  "nudge.mute_today": string;

  // Backend CLI install detection (v0.1.28)
  "backend.installed": string;
  "backend.missing": string;
  "backend.install_open": string;
  "onboarding.cta.next_voice_ime": string;
  "vime.trigger.fn": string;
  "vime.trigger.option": string;
  "vime.trigger.control": string;
  "vime.trigger.right-shift": string;
  "vime.trigger.right-command": string;
  "vime.trigger.right-option": string;
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
  "onboarding.cta.next_capabilities": string;
  "onboarding.cta.finish": string;
  "onboarding.cta.skip": string;
  "onboarding.skip_hint.all_done": string;
  "onboarding.skip_hint.partial": string;
  "onboarding.cheatsheet.title": string;
  "onboarding.cheatsheet.summon": string;
  "onboarding.cheatsheet.clipboard": string;
  "onboarding.cheatsheet.voice_ime": string;
  "onboarding.cheatsheet.click_pet": string;
  "onboarding.cheatsheet.tray_hint": string;
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
  "status.row.officecli.title": string;
  "status.row.officecli.good": string;
  "status.row.officecli.bad": string;
  "status.install.do_it": string;
  "status.install.busy": string;
  "status.install.retry": string;
  "status.install.starting": string;
  "celebrate.title": string;
  "celebrate.subtitle": string;
  "celebrate.try": string;
  "celebrate.dismiss": string;
  "onbinst.title": string;
  "onbinst.lead": string;
  "onbinst.show_cmd": string;
  "onbinst.go": string;
  "onbinst.skip": string;
  "onbinst.continue": string;
  "onbinst.installing": string;
  "onbinst.no_node": string;
  "onbinst.open_node": string;
  "onbinst.browser_group": string;
  "onbinst.office_group": string;
  "onbinst.badge_rec": string;
  "onbinst.badge_adv": string;
  "onbinst.cdp_title": string;
  "onbinst.cdp_desc": string;
  "onbinst.cdp_note": string;
  "onbinst.cdp_enabling": string;
  "onbinst.cdp_done": string;
  "onbinst.ab_title": string;
  "onbinst.ab_desc": string;
  "onbinst.none_title": string;
  "onbinst.none_desc": string;
  "onbinst.tag_nodl": string;
  "onbinst.tag_login": string;
  "onbinst.tag_light": string;
  "onbinst.tag_npm": string;
  "onbinst.tag_dl": string;
  "onbinst.tag_nologin": string;
  "onbinst.office_meta": string;
  "onbinst.go_cdp": string;
  "onbinst.go_ab": string;
  "onbinst.go_office_only": string;
  "onbinst.go_suffix_office": string;
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
  "hub.copy_only": string;
  "hub.delete": string;

  // ── 语言切换 ──
  "lang.zh": string;
  "lang.en": string;
  "lang.menu_title": string;

  // ── 定时任务 / 心跳 (v0.5) ──
  "petmenu.tasks": string;
  "tasks.title": string;
  "tasks.count": string;              // "{n} 个 · {on} 启用"
  "tasks.empty.title": string;
  "tasks.empty.hint": string;
  "tasks.empty.ex1": string;
  "tasks.empty.ex2": string;
  "tasks.new.placeholder": string;
  "tasks.new.parsing": string;
  "tasks.new.failed": string;
  "tasks.next": string;               // "下次 {when}"
  "tasks.last.never": string;
  "tasks.last.running": string;
  "tasks.run_now": string;
  "tasks.edit": string;
  "tasks.delete": string;
  "tasks.delete_confirm": string;     // "删除「{title}」？"
  "tasks.paused": string;
  "tasks.history.title": string;
  "tasks.history.empty": string;
  "tasks.edit.title_label": string;
  "tasks.edit.action_label": string;
  "tasks.edit.freq_label": string;
  "tasks.edit.time_label": string;
  "tasks.edit.every_label": string;   // "每隔（小时）"
  "tasks.edit.days_label": string;
  "tasks.edit.day_label": string;     // "每月几号"
  "tasks.save": string;
  // schedule 频率可读化
  "schedule.daily": string;           // "每天 {time}"
  "schedule.weekday": string;         // "工作日 {time}"
  "schedule.weekly": string;          // "{days} {time}"
  "schedule.interval": string;        // "每隔 {span}"
  "schedule.interval.active": string; // " · {start}–{end}"
  "schedule.monthly": string;         // "每月 {day} 号 {time}"
  "schedule.span_hours": string;      // "{n} 小时"
  "schedule.span_min": string;        // "{n} 分钟"
  "schedule.opt.daily": string;
  "schedule.opt.weekday": string;
  "schedule.opt.weekly": string;
  "schedule.opt.interval": string;
  "schedule.opt.monthly": string;
  "schedule.wd1": string;
  "schedule.wd2": string;
  "schedule.wd3": string;
  "schedule.wd4": string;
  "schedule.wd5": string;
  "schedule.wd6": string;
  "schedule.wd7": string;
  // 相对时间
  "when.today": string;               // "今天 {time}"
  "when.tomorrow": string;            // "明天 {time}"
  "when.other": string;               // "{md} {time}"
  // 确认卡
  "schedule.confirm.title": string;
  "schedule.confirm.what": string;
  "schedule.confirm.delivery": string;
  "schedule.confirm.ok": string;
  "schedule.confirm.edit": string;
  "schedule.confirm.created": string;
  "schedule.confirm.esc": string;
  // 结果气泡
  "schedule.result.tag": string;
  "schedule.result.expand": string;
  "schedule.hint.tag": string;
}

/** 翻译值 —— 字符串 OR (args) => string。后者支持插值。 */
export type TranslationValue =
  | string
  | ((args: Record<string, string | number>) => string);

/** Translation key 联合类型 —— Strings 的所有 key */
export type TranslationKey = keyof Strings;
