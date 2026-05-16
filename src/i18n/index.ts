/**
 * i18n · 极简 hook 实现（v0.1.9）
 *
 * 用法：
 *   const t = useT();
 *   <h1>{t("onboarding.welcome.title")}</h1>
 *   <p>{t("error.generic", { message: e })}</p>
 *
 * 切换语言：
 *   - 自动：进程启动读 Rust `get_language` —— 来自 config.json 的 language 字段
 *   - 手动：托盘菜单 / 设置页调 invoke("save_language", { lang: "en" }) →
 *           Rust 持久化 + 广播 EV_LANG_CHANGED → useT 自动重渲染
 *
 * 性能：单 module 静态 LRU，零依赖（没引 react-i18next 的 50KB），所有翻译同步可用。
 */
import { useEffect, useState, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { zh } from "./zh";
import { en } from "./en";
import type { LangId, Strings, TranslationKey } from "./types";

/** 全部支持语言。新加语言只需要在这里加一项 + 一个 strings 文件即可。 */
export const LANGUAGES: ReadonlyArray<{ id: LangId; label: string; strings: Strings }> = [
  { id: "zh", label: "中文", strings: zh },
  { id: "en", label: "English", strings: en },
] as const;

/** 默认语言 —— 未配置 / Rust 不可用时的 fallback */
export const DEFAULT_LANG: LangId = "zh";

/** 启动时由 system locale 推断的初始 lang —— 用户没明确选过就用它 */
export function detectSystemLang(): LangId {
  const nav = (typeof navigator !== "undefined" ? navigator.language : "") || "";
  if (nav.startsWith("zh")) return "zh";
  if (nav.startsWith("en")) return "en";
  return DEFAULT_LANG;
}

export const EV_LANG_CHANGED = "lang-changed";

// 全局当前语言 + 订阅者集合 —— 这样多个组件用同一个 hook 时只跑一次 listen()
let currentLang: LangId = detectSystemLang();
const subscribers = new Set<(l: LangId) => void>();

function setGlobalLang(l: LangId) {
  if (l === currentLang) return;
  currentLang = l;
  subscribers.forEach(fn => fn(l));
}

// 仅初始化一次：从 Rust 读 + 订阅事件
let initStarted = false;
function ensureInit() {
  if (initStarted) return;
  initStarted = true;
  // 读 Rust 持久化的语言
  invoke<string>("get_language")
    .then(l => {
      if (l && (l === "zh" || l === "en")) setGlobalLang(l as LangId);
    })
    .catch(() => { /* browser-only mode */ });
  // 监听切换事件
  try {
    listen<string>(EV_LANG_CHANGED, e => {
      const v = e.payload;
      if (v === "zh" || v === "en") setGlobalLang(v);
    }).catch(() => {});
  } catch {/* browser-only */}
}

function getStrings(lang: LangId): Strings {
  return LANGUAGES.find(L => L.id === lang)?.strings ?? zh;
}

/** 插值：把 "{name}" 替换成 args.name */
function interpolate(template: string, args?: Record<string, string | number>): string {
  if (!args) return template;
  return template.replace(/\{(\w+)\}/g, (_, k) => {
    const v = args[k];
    return v == null ? `{${k}}` : String(v);
  });
}

/**
 * 翻译 hook —— 组件里这样用：
 *   const t = useT();
 *   t("common.ok")
 *   t("bubble.session_chip", { id: 42, turn: 3 })
 */
export function useT() {
  ensureInit();
  const [lang, setLang] = useState<LangId>(currentLang);
  useEffect(() => {
    subscribers.add(setLang);
    return () => { subscribers.delete(setLang); };
  }, []);

  return useCallback(
    (key: TranslationKey, args?: Record<string, string | number>): string => {
      const strings = getStrings(lang);
      const value = strings[key];
      if (typeof value === "string") return interpolate(value, args);
      // fallback to zh (the canonical source) if key missing
      const fallback = zh[key];
      if (typeof fallback === "string") return interpolate(fallback, args);
      return key; // 显示原始 key 至少能看出哪里缺翻译
    },
    [lang]
  );
}

/** 给非 React 上下文用的同步函数（极少用，主要给 utility 函数）。 */
export function t(key: TranslationKey, args?: Record<string, string | number>): string {
  const strings = getStrings(currentLang);
  const value = strings[key] ?? zh[key];
  return typeof value === "string" ? interpolate(value, args) : key;
}

/** 主动切语言 —— 设置页 / 托盘 调它。 */
export async function setLanguage(lang: LangId): Promise<void> {
  try {
    await invoke("save_language", { lang });
    // Rust 那边会 emit EV_LANG_CHANGED，我们 listen 到就 setGlobalLang
    // 这里也直接更新一次，避免有的窗口没监听上
    setGlobalLang(lang);
  } catch (e) {
    console.warn("setLanguage failed:", e);
    setGlobalLang(lang); // browser-only fallback
  }
}

export function getCurrentLang(): LangId { return currentLang; }
