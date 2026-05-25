/**
 * SettingsView · v0.5.x —— 独立设置窗（?view=settings）。
 *
 * 收拢原先散在托盘里的所有开关：左侧分类导航 + 右侧内容（macOS 系统设置风格）。
 * 设计源：docs/prototypes/settings-page-20260524.html。token 全来自 DESIGN.md。
 *
 * 取值：挂载时 get_settings() 一次读全部 config，外加 get_pet_anchor / get_voice_ime_trigger /
 *   get_pet_identity（枚举类用专用 getter 拿规范字符串，避免 serde 大小写不一致）。
 * 存值：改一项调对应 save_*；运行期联动（皮肤/语言/触发键/sfx）已有广播机制，复用。
 * 诊断分页直接嵌现有 StatusView（能力 / 权限红绿灯 + 装上 / 去授权）。
 */
import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useT, getCurrentLang } from "./i18n";
import StatusView from "./StatusView";
import "./SettingsView.css";

type Cat = "summon" | "voice" | "pet" | "sound" | "general" | "privacy" | "diag";

/* 枚举选项标签 —— 与 Rust 枚举值强耦合，用本地双语映射（值变了这里必须同步）。
   静态界面文案走全局 i18n 表（useT）。zh/en 两列，加日语时这里也要补一列。 */
const LANG = getCurrentLang().startsWith("zh") ? 0 : 1;
const L = (zh: string, en: string) => (LANG === 0 ? zh : en);
const TRIGGER_LABEL: Record<string, string> = {
  "fn": L("按住 fn", "Hold fn"), "option": L("按住 ⌥ option", "Hold ⌥ option"),
  "control": L("按住 ⌃ control", "Hold ⌃ control"), "right-shift": L("按住 右 ⇧", "Hold right ⇧"),
  "right-command": L("按住 右 ⌘", "Hold right ⌘"), "right-option": L("按住 右 ⌥", "Hold right ⌥"),
  "left-shift": L("按住 左 ⇧", "Hold left ⇧"), "left-command": L("按住 左 ⌘", "Hold left ⌘"),
};

/* 召唤组合键漂亮显示：Super+Shift+KeyM → ⌘⇧M */
function prettyChord(chord: string): string {
  if (!chord) return "—";
  return chord.split("+").map((p) => {
    if (p === "Super" || p === "Meta" || p === "Command") return "⌘";
    if (p === "Control" || p === "Ctrl") return "⌃";
    if (p === "Alt" || p === "Option") return "⌥";
    if (p === "Shift") return "⇧";
    return p.replace(/^Key/, "").replace(/^Digit/, "");
  }).join("");
}
/* JS e.code 修饰键 → ImeTrigger 字符串 */
const MOD_TO_TRIGGER: Record<string, string> = {
  ShiftLeft: "left-shift", ShiftRight: "right-shift",
  ControlLeft: "control", ControlRight: "control",
  AltLeft: "option", AltRight: "option",
  MetaLeft: "left-command", MetaRight: "right-command",
};

/* 录制组合键（召唤快捷键）：需 ≥1 修饰键 + 1 主键。Esc 取消。 */
function ShortcutRecorder({ value, onCapture }: { value: string; onCapture: (chord: string) => void }) {
  const [rec, setRec] = useState(false);
  useEffect(() => {
    if (!rec) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault(); e.stopPropagation();
      if (e.key === "Escape") { setRec(false); return; }
      const code = e.code;
      const isMod = /^(Meta|Control|Alt|Shift)/.test(code) || code === "CapsLock";
      if (isMod) return; // 还没按主键，继续等
      const mods: string[] = [];
      if (e.metaKey) mods.push("Super");
      if (e.ctrlKey) mods.push("Control");
      if (e.altKey) mods.push("Alt");
      if (e.shiftKey) mods.push("Shift");
      if (mods.length === 0) return; // 纯主键不接受（全局快捷键必须带修饰键）
      setRec(false);
      onCapture([...mods, code].join("+"));
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [rec, onCapture]);
  return (
    <span className="rec-wrap">
      <span className="kbd">{prettyChord(value)}</span>
      <button type="button" className={`btn ${rec ? "rec-on" : ""}`} onClick={() => setRec((r) => !r)}>
        {rec ? L("按组合键…Esc 取消", "Press keys…Esc") : L("⌨️ 录制", "⌨️ Record")}
      </button>
    </span>
  );
}
/* 录制修饰键（语音触发键）：按下任一修饰键即绑定。Esc 取消。 */
function ModifierRecorder({ onCapture }: { onCapture: (trigger: string) => void }) {
  const [rec, setRec] = useState(false);
  useEffect(() => {
    if (!rec) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault(); e.stopPropagation();
      if (e.key === "Escape") { setRec(false); return; }
      const t = MOD_TO_TRIGGER[e.code];
      if (!t) return; // 等一个修饰键
      setRec(false);
      onCapture(t);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [rec, onCapture]);
  return (
    <button type="button" className={`btn ${rec ? "rec-on" : ""}`} onClick={() => setRec((r) => !r)}>
      {rec ? L("按修饰键…Esc 取消", "Press modifier…Esc") : L("⌨️ 录制", "⌨️ Record")}
    </button>
  );
}
const ANCHOR_LABEL: Record<string, string> = {
  "bottom-right": L("右下", "↘"), "bottom-left": L("左下", "↙"), "top-right": L("右上", "↗"),
  "top-left": L("左上", "↖"), "follow": L("跟随", "Follow"), "hidden": L("隐藏", "Hidden"),
};
const PERSONA_LABEL: Record<string, string> = {
  "warm": L("暖心", "Warm"), "snarky": L("毒舌", "Snarky"), "minimal": L("极简", "Minimal"),
  "companion": L("话痨", "Chatty"), "pro": L("干练", "Pro"), "cheerful": L("元气", "Cheerful"),
  "calm": L("沉稳", "Calm"), "curious": L("好奇", "Curious"), "tsundere": L("傲娇", "Tsundere"),
  "custom": L("自定义", "Custom"),
};

interface Settings {
  shortcut: string;
  backend: string;
  language: string;
  voice_ime_enabled: boolean;
  dictation_tidy: boolean;
  tts_enabled: boolean;
  sfx_enabled: boolean;
  sfx_volume: number;
  clipboard_paused: boolean;
  memory_enabled: boolean;
  autostart: boolean;
  workspace_path: string | null;
}

const BACKENDS = [
  ["claude-cli", "Claude Code CLI"], ["codex-cli", "OpenAI Codex CLI"],
  ["openclaw-cli", "OpenClaw CLI"], ["hermes-agent", "Hermes"],
  ["gemini-cli", "Gemini CLI"], ["opencode-cli", "OpenCode"],
  ["copilot-cli", "Copilot CLI"], ["qwen-code", "Qwen Code"],
] as const;
const TRIGGERS = ["fn", "option", "control", "right-shift", "right-command", "right-option", "left-shift", "left-command"] as const;
const ANCHORS = ["bottom-right", "bottom-left", "top-right", "top-left", "follow", "hidden"] as const;
const PERSONAS = ["warm", "snarky", "minimal", "companion", "pro", "cheerful", "calm", "curious", "tsundere", "custom"] as const;

/* ── 小控件 ── */
function Toggle({ on, onChange }: { on: boolean; onChange: (v: boolean) => void }) {
  return <button type="button" className={`sw ${on ? "on" : ""}`} aria-pressed={on}
    onClick={() => onChange(!on)} />;
}
function Seg<T extends string>({ value, options, onChange }: {
  value: T; options: readonly (readonly [T, string])[]; onChange: (v: T) => void;
}) {
  return <div className="seg">{options.map(([v, label]) => (
    <button key={v} type="button" className={v === value ? "on" : ""} onClick={() => onChange(v)}>{label}</button>
  ))}</div>;
}
function Row({ name, hint, children }: { name: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="row">
      <div className="lbl"><div className="name">{name}</div>{hint && <div className="hint">{hint}</div>}</div>
      <div className="ctrl">{children}</div>
    </div>
  );
}

export default function SettingsView() {
  const t = useT();
  const [cat, setCat] = useState<Cat>("summon");
  const [s, setS] = useState<Settings | null>(null);
  const [anchor, setAnchor] = useState("bottom-right");
  const [trigger, setTrigger] = useState("fn");
  const [name, setName] = useState("");
  const [persona, setPersona] = useState("warm");
  const [custom, setCustom] = useState("");

  // 读身份（名字/性格/自定义）—— picker 窗也能改，要能跨窗同步。
  const loadIdentity = useCallback(() => {
    invoke<{ name: string; personality: string; custom: string }>("get_pet_identity")
      .then((id) => { setName(id.name || ""); setPersona((id.personality || "warm").toLowerCase()); setCustom(id.custom || ""); })
      .catch(() => {});
  }, []);
  // 读全部现值
  const loadAll = useCallback(() => {
    invoke<Settings>("get_settings").then(setS).catch(() => {});
    invoke<string>("get_pet_anchor").then(setAnchor).catch(() => {});
    invoke<string>("get_voice_ime_trigger").then(setTrigger).catch(() => {});
    loadIdentity();
  }, [loadIdentity]);

  // 挂载读一次 + 窗口重新获焦时刷新（在别的窗口/托盘改了设置 → 回到设置窗即同步）。
  useEffect(() => {
    loadAll();
    window.addEventListener("focus", loadAll);
    return () => window.removeEventListener("focus", loadAll);
  }, [loadAll]);

  // picker 窗改名字/性格 → save_pet_identity emit "pet-identity-changed" → 即时刷新（即便没切焦点）。
  useEffect(() => {
    let un: (() => void) | null = null;
    try {
      listen("pet-identity-changed", () => loadIdentity()).then((fn) => { un = fn; }).catch(() => {});
    } catch { /* 浏览器 dev 模式 */ }
    return () => { if (un) un(); };
  }, [loadIdentity]);

  // 局部更新 settings 的某字段 + 调命令
  const set = useCallback(<K extends keyof Settings>(k: K, v: Settings[K]) =>
    setS((prev) => prev ? { ...prev, [k]: v } : prev), []);

  const saveIdentity = useCallback((n: string, p: string, c: string) => {
    invoke("save_pet_identity", { name: n, personality: p, custom: c }).catch(() => {});
  }, []);

  // v0.7 · 加词功能已删 —— SenseVoice（CTC）不支持 hotwords biasing，加词无效。

  if (!s) return <div className="settings-root"><div className="loading">…</div></div>;

  const NAV: [Cat, string, string][] = [
    ["summon", "⌘", t("set.cat.summon")],
    ["voice", "🎙️", t("set.cat.voice")],
    ["pet", "🐭", t("set.cat.pet")],
    ["sound", "🔊", t("set.cat.sound")],
    ["general", "⚙️", t("set.cat.general")],
    ["privacy", "🔒", t("set.cat.privacy")],
    ["diag", "🩺", t("set.cat.diag")],
  ];

  return (
    <div className="settings-root">
      <div className="sidebar">
        {NAV.map(([c, ico, label]) => (
          <div key={c}>
            {c === "diag" && <div className="nav-sep" />}
            <button className={`nav ${cat === c ? "on" : ""}`} onClick={() => setCat(c)}>
              <span className="ico">{ico}</span>{label}
            </button>
          </div>
        ))}
      </div>

      <div className="content">
        {cat === "summon" && (
          <section>
            <h2>{t("set.cat.summon")}</h2>
            <div className="group">
              <Row name={t("set.shortcut")} hint={t("set.shortcut.hint")}>
                <ShortcutRecorder value={s.shortcut} onCapture={(chord) => {
                  invoke("set_summon_shortcut", { shortcut: chord })
                    .then(() => invoke<Settings>("get_settings").then(setS)).catch(() => {});
                }} />
              </Row>
              <Row name={t("set.backend")} hint={t("set.backend.hint")}>
                <select className="sel" value={s.backend}
                  onChange={(e) => { set("backend", e.target.value); invoke("save_backend", { backend: e.target.value }).catch(() => {}); }}>
                  {BACKENDS.map(([v, l]) => <option key={v} value={v}>{l}</option>)}
                </select>
              </Row>
            </div>
          </section>
        )}

        {cat === "voice" && (
          <section>
            <h2>{t("set.cat.voice")}</h2>
            <div className="group">
              <Row name={t("set.vime")} hint={t("set.vime.hint")}>
                <Toggle on={s.voice_ime_enabled} onChange={(v) => { set("voice_ime_enabled", v); invoke("save_voice_ime", { enabled: v }).catch(() => {}); }} />
              </Row>
              <Row name={t("set.trigger")} hint={t("set.trigger.hint")}>
                <span className="rec-wrap">
                  <select className="sel" value={trigger}
                    onChange={(e) => { setTrigger(e.target.value); invoke("save_voice_ime_trigger", { trigger: e.target.value }).catch(() => {}); }}>
                    {TRIGGERS.map((v) => <option key={v} value={v}>{TRIGGER_LABEL[v]}</option>)}
                  </select>
                  <ModifierRecorder onCapture={(tr) => { setTrigger(tr); invoke("save_voice_ime_trigger", { trigger: tr }).catch(() => {}); }} />
                </span>
              </Row>
              <Row name={t("set.tidy")} hint={t("set.tidy.hint")}>
                <Toggle on={s.dictation_tidy} onChange={(v) => { set("dictation_tidy", v); invoke("save_dictation_tidy", { enabled: v }).catch(() => {}); }} />
              </Row>
            </div>
          </section>
        )}

        {cat === "pet" && (
          <section>
            <h2>{t("set.cat.pet")}</h2>
            <div className="group">
              <Row name={t("set.skin")} hint={t("set.skin.hint")}>
                <button className="btn" onClick={() => invoke("open_picker_window").catch(() => {})}>{t("set.skin.btn")}</button>
              </Row>
              <Row name={t("set.anchor")} hint={t("set.anchor.hint")}>
                <Seg value={anchor} options={ANCHORS.map((a) => [a, ANCHOR_LABEL[a]]) as [string, string][]}
                  onChange={(v) => { setAnchor(v); invoke("save_pet_anchor", { anchor: v }).catch(() => {}); }} />
              </Row>
              <Row name={t("set.name")} hint={t("set.name.hint")}>
                <input className="txt" value={name} placeholder={t("set.name.ph")}
                  onChange={(e) => setName(e.target.value)}
                  onBlur={() => saveIdentity(name, persona, custom)} />
              </Row>
              <Row name={t("set.persona")} hint={t("set.persona.hint")}>
                <select className="sel" value={persona}
                  onChange={(e) => { setPersona(e.target.value); saveIdentity(name, e.target.value, custom); }}>
                  {PERSONAS.map((p) => <option key={p} value={p}>{PERSONA_LABEL[p]}</option>)}
                </select>
              </Row>
            </div>
          </section>
        )}

        {cat === "sound" && (
          <section>
            <h2>{t("set.cat.sound")}</h2>
            <div className="group">
              <Row name={t("set.tts")} hint={t("set.tts.hint")}>
                <Toggle on={s.tts_enabled} onChange={(v) => { set("tts_enabled", v); invoke("save_tts", { enabled: v }).catch(() => {}); }} />
              </Row>
              <Row name={t("set.sfx")} hint={t("set.sfx.hint")}>
                <Toggle on={s.sfx_enabled} onChange={(v) => { set("sfx_enabled", v); invoke("save_sfx", { enabled: v, volume: s.sfx_volume }).catch(() => {}); }} />
              </Row>
              <Row name={t("set.sfx.vol")}>
                <input className="slider" type="range" min={0} max={100} value={Math.round(s.sfx_volume * 100)}
                  onChange={(e) => set("sfx_volume", Number(e.target.value) / 100)}
                  onMouseUp={() => invoke("save_sfx", { enabled: s.sfx_enabled, volume: s.sfx_volume }).catch(() => {})} />
              </Row>
            </div>
          </section>
        )}

        {cat === "general" && (
          <section>
            <h2>{t("set.cat.general")}</h2>
            <div className="group">
              <Row name={t("set.lang")} hint={t("set.lang.hint")}>
                <Seg value={s.language} options={[["zh", "中文"], ["en", "English"]] as [string, string][]}
                  onChange={(v) => { set("language", v); invoke("save_language", { lang: v }).catch(() => {}); }} />
              </Row>
              <Row name={t("set.autostart")} hint={t("set.autostart.hint")}>
                <Toggle on={s.autostart} onChange={(v) => { set("autostart", v); invoke("set_autostart", { enable: v }).catch(() => {}); }} />
              </Row>
              <Row name={t("set.workspace")} hint={s.workspace_path || t("set.workspace.hint")}>
                <button className="btn" onClick={() => invoke("pick_workspace_folder").then(() =>
                  invoke<Settings>("get_settings").then(setS)).catch(() => {})}>{t("set.workspace.btn")}</button>
              </Row>
            </div>
          </section>
        )}

        {cat === "privacy" && (
          <section>
            <h2>{t("set.cat.privacy")}</h2>
            <div className="desc">{t("set.privacy.desc")}</div>
            <div className="group">
              <Row name={t("set.clip")} hint={t("set.clip.hint")}>
                <Toggle on={!s.clipboard_paused} onChange={(v) => { set("clipboard_paused", !v); invoke("save_clipboard_paused", { paused: !v }).catch(() => {}); }} />
              </Row>
              <Row name={t("set.memory")} hint={t("set.memory.hint")}>
                <Toggle on={s.memory_enabled} onChange={(v) => { set("memory_enabled", v); invoke("save_memory_enabled", { enabled: v }).catch(() => {}); }} />
              </Row>
              <Row name={t("set.memory.manage")} hint={t("set.memory.manage.hint")}>
                <button className="btn" onClick={() => invoke("open_memory_window").catch(() => {})}>{t("set.memory.manage.btn")}</button>
              </Row>
            </div>
          </section>
        )}

        {cat === "diag" && (
          <section className="diag-embed">
            <h2>{t("set.cat.diag")}</h2>
            <div className="desc">{t("set.diag.desc")}</div>
            <StatusView />
          </section>
        )}
      </div>
    </div>
  );
}
