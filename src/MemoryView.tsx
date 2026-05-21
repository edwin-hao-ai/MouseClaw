/**
 * MemoryView · v0.4.4
 *
 * 「桌宠记得的事」窗口(?view=memory)。双层(仿 ChatGPT memory):
 *   - 📌 关于你 = 画像层(insight + preference,reflection 蒸馏出来的,可删)
 *   - 🕑 历史   = 情景层(每轮 turn,可搜可删)
 * 顶部「记忆」开关 = 暂停(ChatGPT Temporary:既不读也不写)。
 * 设计:docs/design/pet-identity-and-memory-20260521.md · prototype:docs/prototypes/pet-memory-20260521.html
 */
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useT } from "./i18n";
import "./MemoryView.css";

interface Insight { id: number; kind: string; text: string; confidence: number }
interface Pref { id: number; key: string; value: string; confidence: number }
interface Turn { id: number; ts: number; app: string | null; role: string; summary: string; importance: number }

function timeAgo(ts: number, lang: string): string {
  const d = Math.max(0, Math.floor(Date.now() / 1000) - ts);
  const en = lang === "en";
  if (d < 60) return en ? "just now" : "刚刚";
  if (d < 3600) return en ? `${Math.floor(d / 60)}m ago` : `${Math.floor(d / 60)}分钟前`;
  if (d < 86400) return en ? `${Math.floor(d / 3600)}h ago` : `${Math.floor(d / 3600)}小时前`;
  return en ? `${Math.floor(d / 86400)}d ago` : `${Math.floor(d / 86400)}天前`;
}

export default function MemoryView() {
  const t = useT();
  const [tab, setTab] = useState<"profile" | "history">("profile");
  const [paused, setPaused] = useState(false);
  const [insights, setInsights] = useState<Insight[]>([]);
  const [prefs, setPrefs] = useState<Pref[]>([]);
  const [turns, setTurns] = useState<Turn[]>([]);
  const [search, setSearch] = useState("");

  const loadProfile = useCallback(() => {
    invoke<{ insights: Insight[]; preferences: Pref[] }>("memory_get_profile")
      .then(p => { setInsights(p.insights ?? []); setPrefs(p.preferences ?? []); })
      .catch(() => {});
  }, []);
  const loadTurns = useCallback((q: string) => {
    invoke<Turn[]>("memory_list_turns", { query: q || null })
      .then(rows => setTurns(rows ?? []))
      .catch(() => {});
  }, []);

  useEffect(() => {
    invoke<{ enabled: boolean; paused: boolean }>("memory_get_settings")
      .then(s => setPaused(!!s.paused)).catch(() => {});
    loadProfile();
    loadTurns("");
  }, [loadProfile, loadTurns]);

  const togglePaused = () => {
    const next = !paused;
    setPaused(next);
    invoke("memory_set_paused", { paused: next }).catch(() => {});
  };

  const delTurn = (id: number) => {
    invoke("memory_delete_turn", { id }).catch(() => {});
    setTurns(ts => ts.filter(t => t.id !== id));
  };
  const delProfile = (kind: "insight" | "preference", id: number) => {
    invoke("memory_delete_profile_item", { kind, id }).catch(() => {});
    if (kind === "insight") setInsights(xs => xs.filter(x => x.id !== id));
    else setPrefs(xs => xs.filter(x => x.id !== id));
  };
  const clearAll = () => {
    if (!window.confirm(t("memory.clear_confirm"))) return;
    invoke("memory_clear_all").catch(() => {});
    setInsights([]); setPrefs([]); setTurns([]);
  };

  const onSearch = (q: string) => { setSearch(q); loadTurns(q); };
  const lang = navigator.language?.startsWith("en") ? "en" : "zh";
  const profileEmpty = insights.length === 0 && prefs.length === 0;

  return (
    <div className="mem-root">
      <header className="mem-header">
        <h1>🧠 {t("memory.title")}</h1>
        <button
          type="button"
          className={`mem-pause ${paused ? "off" : ""}`}
          onClick={togglePaused}
          title={t("memory.pause_label")}
        >
          <span>{t("memory.pause_label")}</span>
          <span className="mem-sw" aria-hidden />
        </button>
      </header>

      {paused && <div className="mem-banner">{t("memory.paused_banner")}</div>}

      <div className="mem-tabs">
        <button type="button" className={`mem-tab ${tab === "profile" ? "sel" : ""}`}
          onClick={() => setTab("profile")}>{t("memory.tab.profile")}</button>
        <button type="button" className={`mem-tab ${tab === "history" ? "sel" : ""}`}
          onClick={() => setTab("history")}>{t("memory.tab.history")}</button>
      </div>

      {tab === "profile" && (
        <section className="mem-pane">
          <p className="mem-intro">{t("memory.profile.intro")}</p>
          {profileEmpty && <div className="mem-empty">{t("memory.profile.empty")}</div>}
          {insights.map(i => (
            <div className="mem-chip" key={`i-${i.id}`}>
              <div className="mem-chip-body">
                <div className="mem-chip-k">{i.kind}</div>
                <div className="mem-chip-v">{i.text}</div>
              </div>
              <div className="mem-conf"><i style={{ width: `${Math.round(i.confidence * 100)}%` }} /></div>
              <button type="button" className="mem-x" title={t("memory.delete")}
                onClick={() => delProfile("insight", i.id)}>✕</button>
            </div>
          ))}
          {prefs.map(p => (
            <div className="mem-chip" key={`p-${p.id}`}>
              <div className="mem-chip-body">
                <div className="mem-chip-k">{p.key}</div>
                <div className="mem-chip-v">{p.value}</div>
              </div>
              <div className="mem-conf"><i style={{ width: `${Math.round(p.confidence * 100)}%` }} /></div>
              <button type="button" className="mem-x" title={t("memory.delete")}
                onClick={() => delProfile("preference", p.id)}>✕</button>
            </div>
          ))}
        </section>
      )}

      {tab === "history" && (
        <section className="mem-pane">
          <input className="mem-search" value={search}
            placeholder={t("memory.history.search")}
            onChange={e => onSearch(e.target.value)} />
          {turns.length === 0 && <div className="mem-empty">{t("memory.history.empty")}</div>}
          {turns.map(tn => (
            <div className="mem-turn" key={tn.id}>
              {tn.app && <span className="mem-app">{tn.app}</span>}
              <div className="mem-turn-mid">
                <div className="mem-turn-sum">{tn.summary}</div>
                <div className="mem-turn-meta">
                  {timeAgo(tn.ts, lang)} · <span className="mem-imp">
                    {"●".repeat(tn.importance).slice(0, 5)}{"○".repeat(Math.max(0, 5 - tn.importance))}
                  </span> {t("memory.importance")}
                </div>
              </div>
              <button type="button" className="mem-x" title={t("memory.delete")}
                onClick={() => delTurn(tn.id)}>✕</button>
            </div>
          ))}
        </section>
      )}

      <footer className="mem-footer">
        <span className="mem-local">{t("memory.local_note")}</span>
        <button type="button" className="mem-clear" onClick={clearAll}>{t("memory.clear_all")}</button>
      </footer>
    </div>
  );
}
