//! 长期记忆 · 本地 SQLite(无向量库) · 三层(画像 / 情景 / 图谱)。
//! 设计:docs/design/pet-identity-and-memory-20260521.md
//!
//! 取舍:① 无向量模型 —— 检索靠 关系表 + 时近/重要度/token 重叠 在 Rust 内打分
//! (零额外常驻内存;FTS5/BM25 留 v1.1)。② 记录零额外 LLM —— `record_turn` 只写库,
//! LLM 蒸馏挪到空闲 `run_reflection`(省 token)。③ backend 无关 —— reflection 走
//! `backend::ask_text_only`,注入走 `claude_cli::build_prompt`,4 后端 + 未来直连 LLM 通吃。
//! ④ 隐私 —— 只存召唤那一刻已有素材(app/文本/截图路径),纯本地,双开关(enabled/paused)。

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use once_cell::sync::Lazy;
use rusqlite::{params, Connection};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// 单连接 + Mutex 串行访问(记忆读写量小,SELECT < 10ms)。Connection 是 Send。
static MEM: Lazy<Mutex<Option<Connection>>> = Lazy::new(|| Mutex::new(None));

/// 最近一次 retrieve_block 实际注入的记忆条目 —— pipeline 读后 emit 给前端,
/// 让用户看到「这次回复用了哪几条」+ 当场删错的(信任 + 可控)。
#[derive(Clone, serde::Serialize)]
pub struct UsedItem {
    pub kind: String, // profile(insight) | preference | turn —— 决定删除走哪张表
    pub id: i64,
    pub text: String,
}
static LAST_ITEMS: Lazy<Mutex<Vec<UsedItem>>> = Lazy::new(|| Mutex::new(Vec::new()));

/// reflection 串行闸 —— 防 pipeline(满 6 条触发) 与 idle 定时器并发 spawn `run_reflection`,
/// 两者在 fetch_unprocessed→acquire 窗口里拿到同一批 turn → 重复写 insight/边。一次只许一个反思在跑。
static REFLECTING: AtomicBool = AtomicBool::new(false);

/// RAII 认领 reflection。已有反思在跑 → `try_acquire()` 返回 None,调用方直接 return。
struct ReflectGuard;
impl ReflectGuard {
    fn try_acquire() -> Option<Self> {
        REFLECTING
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| ReflectGuard)
    }
}
impl Drop for ReflectGuard {
    fn drop(&mut self) {
        REFLECTING.store(false, Ordering::SeqCst);
    }
}

/// 读并清空(pipeline 在 AI 调用后读一次)。
pub fn take_last_used_items() -> Vec<UsedItem> {
    match LAST_ITEMS.lock() {
        Ok(mut g) => std::mem::take(&mut *g),
        Err(_) => Vec::new(),
    }
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS memory_turn (
  id          INTEGER PRIMARY KEY,
  ts          INTEGER NOT NULL,
  app         TEXT,
  role        TEXT NOT NULL,      -- user / assistant
  text        TEXT NOT NULL,
  screenshot  TEXT,
  summary     TEXT,               -- reflection 回填的一行摘要
  importance  INTEGER DEFAULT 3,  -- 1-5,写入期启发式 + reflection 可调
  processed   INTEGER DEFAULT 0   -- 0 = reflection 还没消化
);
CREATE INDEX IF NOT EXISTS idx_turn_ts ON memory_turn(ts);
CREATE INDEX IF NOT EXISTS idx_turn_processed ON memory_turn(processed);

CREATE TABLE IF NOT EXISTS entity (
  id         INTEGER PRIMARY KEY,
  kind       TEXT NOT NULL,       -- project | app | file | topic | tool | person
  name       TEXT NOT NULL,
  first_seen INTEGER,
  last_seen  INTEGER,
  freq       INTEGER DEFAULT 1,
  UNIQUE(kind, name)
);

CREATE TABLE IF NOT EXISTS edge (
  src    INTEGER,
  dst    INTEGER,
  kind   TEXT,                    -- mentions | co_occurs | followed_by | about
  weight REAL DEFAULT 1,
  ts     INTEGER
);

-- 偏好:带时序有效期(Zep 思路:更新=旧的失效,不删,留史)
CREATE TABLE IF NOT EXISTS preference (
  id         INTEGER PRIMARY KEY,
  key        TEXT NOT NULL,
  value      TEXT NOT NULL,
  confidence REAL DEFAULT 0.6,
  valid_from INTEGER,
  valid_to   INTEGER             -- NULL = 当前有效
);

-- 画像/洞察:reflection 的产物(进化沉淀)
CREATE TABLE IF NOT EXISTS insight (
  id         INTEGER PRIMARY KEY,
  kind       TEXT NOT NULL,       -- profile | project_state | pattern
  text       TEXT NOT NULL,
  confidence REAL DEFAULT 0.6,
  valid_from INTEGER,
  valid_to   INTEGER,            -- NULL = 仍有效
  updated    INTEGER
);
"#;

fn db_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".mouseclaw/memory.db"))
}

/// 首次启动建库。失败只 log —— 记忆是增强项,挂了不能拖垮主流程。
pub fn init() -> Result<()> {
    let path = db_path().context("HOME not set")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let conn = Connection::open(&path).with_context(|| format!("open {}", path.display()))?;
    conn.execute_batch(SCHEMA).context("create memory schema")?;
    *MEM.lock().map_err(|_| anyhow!("memory lock poisoned"))? = Some(conn);
    println!("[mouseclaw] memory db ready → {}", path.display());
    Ok(())
}

fn with_db<T>(f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Result<T> {
    let guard = MEM.lock().map_err(|_| anyhow!("memory lock poisoned"))?;
    let conn = guard.as_ref().context("memory db not initialized")?;
    f(conn).map_err(|e| anyhow!("memory db: {e}"))
}

/// 记忆是否当前可用(总开关 + 暂停)。读/写都先过它。
fn enabled() -> bool {
    let cfg = crate::config::Config::load();
    cfg.memory_enabled && !cfg.memory_paused
}

/// v0.4.4 · 记忆默认开 → 首次启动一次性透明告知(本地存、可看可删)。
/// marker file 兜底,弹过不再弹。仿 `cli_install::maybe_hint_upgrade`。
pub fn maybe_show_memory_intro(app: tauri::AppHandle) {
    use tauri::Emitter;
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        let cfg = crate::config::Config::load();
        if !cfg.onboarded || !cfg.memory_enabled {
            return; // 新用户走 onboarding;关了记忆就别提
        }
        let marker = match std::env::var_os("HOME") {
            Some(h) => PathBuf::from(h).join(".mouseclaw").join("memory_intro_shown"),
            None => return,
        };
        if marker.exists() {
            return; // 弹过了
        }
        let en = cfg.language != "zh";
        let message = if en {
            "🧠 I'll remember what we do together — all stored locally, view or delete it anytime.".to_string()
        } else {
            "🧠 我会记得我们一起做过的事 —— 全部存在你本地,随时可看可删。".to_string()
        };
        let cta_label = if en { "View".to_string() } else { "看看".to_string() };
        let payload = crate::events::NudgePayload {
            kind: crate::events::NudgeKind::MemoryIntro,
            message,
            cta_label: Some(cta_label),
            cta_action: Some("open-memory".into()),
        };
        if let Err(e) = app.emit(crate::events::EV_NUDGE, payload) {
            eprintln!("[mouseclaw] memory-intro emit failed: {e}");
            return;
        }
        if let Some(dir) = marker.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&marker, b"1");
        println!("[mouseclaw] memory intro shown");
    });
}

// ── 写入:记录一轮 turn(零额外 LLM) ──────────────────────────────────

fn importance_heuristic(text: &str) -> i64 {
    let len = text.chars().count();
    let mut s = 2;
    if len > 80 { s += 1; }
    if len > 240 { s += 1; }
    if text.contains('?') || text.contains('？') { s += 1; }
    s.min(5)
}

/// 记一轮 turn。`role` = "user" / "assistant"。返回 rowid(0 = 未记录/禁用)。
pub fn record_turn(role: &str, text: &str, app: Option<&str>, screenshot: Option<&str>) -> i64 {
    if !enabled() || text.trim().is_empty() {
        return 0;
    }
    let ts = Utc::now().timestamp();
    let importance = importance_heuristic(text);
    let r = with_db(|c| {
        c.execute(
            "INSERT INTO memory_turn (ts, app, role, text, screenshot, importance, processed)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
            params![ts, app, role, text, screenshot, importance],
        )?;
        Ok(c.last_insert_rowid())
    });
    match r {
        Ok(id) => id,
        Err(e) => {
            eprintln!("[mouseclaw] memory record_turn failed: {e:#}");
            0
        }
    }
}

// ── 读取:拼一段记忆 block 注入 prompt(纯 SQL + Rust 打分,零 LLM) ──────

/// 把 query 切成 token(>=2 char)。CJK 无空格时整句也算一个 token(子串匹配兜底)。
fn query_tokens(query: &str) -> Vec<String> {
    let mut toks: Vec<String> = query
        .split(|c: char| c.is_whitespace() || c.is_ascii_punctuation()
            || matches!(c, '，' | '。' | '、' | '？' | '！' | '：' | '；' | '\u{201c}' | '\u{201d}' | '（' | '）'))
        .map(|s| s.trim().to_string())
        .filter(|s| s.chars().count() >= 2)
        .collect();
    let whole = query.trim();
    if whole.chars().count() >= 2 && !toks.iter().any(|t| t == whole) {
        toks.push(whole.to_string());
    }
    toks.truncate(8);
    toks
}

fn time_ago(ts: i64) -> String {
    let now = Utc::now().timestamp();
    let d = (now - ts).max(0);
    if d < 60 { "刚刚".into() }
    else if d < 3600 { format!("{}分钟前", d / 60) }
    else if d < 86400 { format!("{}小时前", d / 3600) }
    else { format!("{}天前", d / 86400) }
}

struct ScoredTurn { id: i64, ts: i64, app: Option<String>, snippet: String, score: f64 }

fn snippet_of(summary: Option<String>, text: &str) -> String {
    summary.unwrap_or_else(|| {
        let s: String = text.chars().take(60).collect();
        if text.chars().count() > 60 { format!("{s}…") } else { s }
    })
}

/// 当前画像(always-on):有效 insight + 有效 preference,带 id/kind(供前端审查删除)。
fn collect_profile_items(c: &Connection) -> rusqlite::Result<Vec<UsedItem>> {
    let mut out = Vec::new();
    let mut st = c.prepare(
        "SELECT id, text FROM insight WHERE valid_to IS NULL AND kind IN ('profile','pattern')
         ORDER BY confidence DESC LIMIT 6",
    )?;
    let rows = st.query_map([], |r|
        Ok(UsedItem { kind: "insight".into(), id: r.get(0)?, text: r.get(1)? }))?;
    for r in rows { out.push(r?); }
    let mut sp = c.prepare(
        "SELECT id, key, value FROM preference WHERE valid_to IS NULL ORDER BY confidence DESC LIMIT 6",
    )?;
    let prefs = sp.query_map([], |r| {
        let key: String = r.get(1)?;
        let value: String = r.get(2)?;
        Ok(UsedItem { kind: "preference".into(), id: r.get(0)?, text: format!("{key}:{value}") })
    })?;
    for p in prefs { out.push(p?); }
    Ok(out)
}

/// 取最近 N 条 turn,在 Rust 内按 token 重叠 + 同 app + 时近 + 重要度 打分,选 top。
fn collect_relevant(c: &Connection, query: &str, app: Option<&str>, limit: usize)
    -> rusqlite::Result<Vec<ScoredTurn>>
{
    let toks = query_tokens(query);
    let now = Utc::now().timestamp();
    let mut st = c.prepare(
        "SELECT id, ts, app, role, text, summary, importance FROM memory_turn
         ORDER BY ts DESC LIMIT 120",
    )?;
    let rows = st.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, Option<String>>(5)?,
            r.get::<_, i64>(6)?,
        ))
    })?;
    let mut scored: Vec<ScoredTurn> = Vec::new();
    for row in rows {
        let (id, ts, tapp, _role, text, summary, importance) = row?;
        let hay = summary.clone().unwrap_or_else(|| text.clone());
        let hay_l = hay.to_lowercase();
        // relevance:命中的 token 数
        let hits = toks.iter().filter(|t| hay_l.contains(t.to_lowercase().as_str())).count();
        let same_app = matches!((app, tapp.as_deref()), (Some(a), Some(b)) if a == b);
        // 质量闸(防"帮倒忙"):必须有相关性信号 —— token 命中 或 同 app。
        // 纯靠时近(最近但无关)不注入,否则给 AI 喂噪声反而拉低回答质量。
        if hits == 0 && !same_app {
            continue;
        }
        let mut score = hits as f64 * 3.0;
        if same_app { score += 1.5; }
        // 时近(指数衰减,7 天半衰期)
        let age_days = ((now - ts).max(0) as f64) / 86400.0;
        score += 2.0 * 0.5_f64.powf(age_days / 7.0);
        // 重要度
        score += importance as f64 * 0.4;
        scored.push(ScoredTurn { id, ts, app: tapp, snippet: snippet_of(summary, &text), score });
    }
    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    Ok(scored)
}

/// 知识图谱遍历:种子实体(名字命中 query / app)→ 1 跳邻居(entity↔entity 边)→
/// 这些实体被提及的 turn(mentions 边)。全在 Rust 内算(对中文鲁棒,动态 IN 只用 i64,
/// 无注入面)。让"以前在这个项目/这个话题下发生过什么"能被召回 —— 这才是图带来的价值。
fn collect_graph(c: &Connection, query: &str, app: Option<&str>, limit: usize)
    -> rusqlite::Result<Vec<ScoredTurn>>
{
    let toks: Vec<String> = query_tokens(query).iter().map(|t| t.to_lowercase()).collect();
    let app_l = app.map(|a| a.to_lowercase());
    // 1. 载入实体
    let mut est = c.prepare("SELECT id, name FROM entity ORDER BY last_seen DESC LIMIT 400")?;
    let erows = est.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
    let mut id_name: Vec<(i64, String)> = Vec::new();
    for e in erows { id_name.push(e?); }
    if id_name.is_empty() { return Ok(Vec::new()); }
    // 2. 种子:名字被 query token / app 命中
    let mut seeds: HashSet<i64> = HashSet::new();
    for (id, name) in &id_name {
        let nl = name.to_lowercase();
        if nl.chars().count() < 2 { continue; }
        let by_tok = toks.iter().any(|t| nl.contains(t.as_str()) || t.contains(nl.as_str()));
        let by_app = app_l.as_deref().map(|a| a.contains(nl.as_str()) || nl.contains(a)).unwrap_or(false);
        if by_tok || by_app { seeds.insert(*id); }
    }
    if seeds.is_empty() { return Ok(Vec::new()); }
    // 3. 载入边
    let mut xst = c.prepare("SELECT src, dst, kind FROM edge LIMIT 4000")?;
    let xrows = xst.query_map([], |r|
        Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)))?;
    let mut edges: Vec<(i64, i64, String)> = Vec::new();
    for x in xrows { edges.push(x?); }
    // 4. 1 跳邻居(entity↔entity)
    let mut nodes = seeds.clone();
    for (s, d, k) in &edges {
        if k == "co_occurs" || k == "about" {
            if seeds.contains(s) { nodes.insert(*d); }
            if seeds.contains(d) { nodes.insert(*s); }
        }
    }
    // 5. 提及这些实体的 turn(mentions 边:src=turn, dst=entity)
    let mut turn_ids: HashSet<i64> = HashSet::new();
    for (s, d, k) in &edges {
        if k == "mentions" && nodes.contains(d) { turn_ids.insert(*s); }
    }
    if turn_ids.is_empty() { return Ok(Vec::new()); }
    // 6. 取这些 turn(i64 拼 IN,无注入面)
    let csv = turn_ids.iter().take(40).map(|i| i.to_string()).collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT id, ts, app, summary, text, importance FROM memory_turn WHERE id IN ({csv}) ORDER BY ts DESC"
    );
    let now = Utc::now().timestamp();
    let mut tst = c.prepare(&sql)?;
    let trows = tst.query_map([], |r| Ok((
        r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, Option<String>>(2)?,
        r.get::<_, Option<String>>(3)?, r.get::<_, String>(4)?, r.get::<_, i64>(5)?,
    )))?;
    let mut out = Vec::new();
    for row in trows {
        let (id, ts, tapp, summary, text, importance) = row?;
        let age_days = ((now - ts).max(0) as f64) / 86400.0;
        let score = 2.5 + 2.0 * 0.5_f64.powf(age_days / 7.0) + importance as f64 * 0.4;
        out.push(ScoredTurn { id, ts, app: tapp, snippet: snippet_of(summary, &text), score });
    }
    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(limit);
    Ok(out)
}

/// 注入 prompt 的记忆块。无内容 / 禁用 / 暂停 → None。
pub fn retrieve_block(query: &str, app: Option<&str>) -> Option<String> {
    if let Ok(mut g) = LAST_ITEMS.lock() { g.clear(); }
    if !enabled() {
        return None;
    }
    let res: Result<Option<String>> = with_db(|c| {
        let profile = collect_profile_items(c).unwrap_or_default();
        // 情景层(token/时近/同app 打分)+ 图谱层(实体邻居→相关 turn),合并去重。
        let mut cands = collect_relevant(c, query, app, 8).unwrap_or_default();
        cands.extend(collect_graph(c, query, app, 6).unwrap_or_default());
        cands.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        let mut seen: HashSet<i64> = HashSet::new();
        let mut turns: Vec<ScoredTurn> = Vec::new();
        for t in cands {
            if seen.insert(t.id) {
                turns.push(t);
                if turns.len() >= 5 { break; }
            }
        }
        if profile.is_empty() && turns.is_empty() {
            return Ok(None);
        }
        // 记下实际注入的条目(前端可审查/删除)
        let mut items: Vec<UsedItem> = profile.clone();
        for t in &turns {
            items.push(UsedItem { kind: "turn".into(), id: t.id, text: t.snippet.clone() });
        }
        if let Ok(mut g) = LAST_ITEMS.lock() { *g = items; }
        let mut s = String::from("[记忆 · 仅供参考,以当前任务为准]\n");
        for p in &profile {
            s.push_str("- ");
            s.push_str(&p.text);
            s.push('\n');
        }
        for t in &turns {
            let app_part = t.app.as_deref().map(|a| format!("在{a} ")).unwrap_or_default();
            s.push_str(&format!("- {} {}{}\n", time_ago(t.ts), app_part, t.snippet));
        }
        s.push_str("[/记忆]");
        // token 预算:截断到 ~900 字符
        if s.chars().count() > 900 {
            let cut: String = s.chars().take(900).collect();
            s = format!("{cut}…\n[/记忆]");
        }
        Ok(Some(s))
    });
    res.ok().flatten()
}

// ── reflection(P2.5):空闲时把 unprocessed turn 蒸馏成画像 ────────────

struct BatchTurn { id: i64, role: String, text: String }

fn fetch_unprocessed(c: &Connection, limit: usize) -> rusqlite::Result<Vec<BatchTurn>> {
    let mut st = c.prepare(
        "SELECT id, role, text FROM memory_turn WHERE processed = 0 ORDER BY ts ASC LIMIT ?1",
    )?;
    let rows = st.query_map(params![limit as i64], |r| {
        Ok(BatchTurn { id: r.get(0)?, role: r.get(1)?, text: r.get(2)? })
    })?;
    let mut v = Vec::new();
    for r in rows { v.push(r?); }
    Ok(v)
}

pub fn unprocessed_count() -> i64 {
    with_db(|c| {
        c.query_row("SELECT COUNT(*) FROM memory_turn WHERE processed = 0", [], |r| r.get(0))
    })
    .unwrap_or(0)
}

/// 遗忘 / 剪枝 —— 防止长期无限增长(设计 §2.7)。定时器里周期跑。
/// 留最近+高重要度的 ~2000 条 turn;清掉孤儿提及边;归档 60 天前已失效的画像。
pub fn prune() {
    let cutoff = Utc::now().timestamp() - 60 * 86400;
    let _ = with_db(|c| {
        c.execute(
            "DELETE FROM memory_turn WHERE id NOT IN (
               SELECT id FROM memory_turn ORDER BY importance DESC, ts DESC LIMIT 2000)",
            [],
        )?;
        // 指向已删 turn 的提及边一起清
        c.execute(
            "DELETE FROM edge WHERE kind='mentions' AND src NOT IN (SELECT id FROM memory_turn)",
            [],
        )?;
        // 60 天前就失效的旧画像/偏好 —— 留史足够久,再老就归档
        c.execute("DELETE FROM insight WHERE valid_to IS NOT NULL AND valid_to < ?1", params![cutoff])?;
        c.execute("DELETE FROM preference WHERE valid_to IS NOT NULL AND valid_to < ?1", params![cutoff])?;
        Ok(())
    });
}

#[derive(serde::Deserialize, Default)]
struct Extraction {
    #[serde(default)] insights: Vec<InsightDto>,
    #[serde(default)] preferences: Vec<PrefDto>,
    #[serde(default)] summaries: Vec<SummaryDto>,
    #[serde(default)] entities: Vec<EntityDto>,
    #[serde(default)] relations: Vec<RelationDto>,
}
#[derive(serde::Deserialize)]
struct InsightDto { kind: String, text: String, #[serde(default)] confidence: Option<f64> }
#[derive(serde::Deserialize)]
struct PrefDto { key: String, value: String, #[serde(default)] confidence: Option<f64> }
#[derive(serde::Deserialize)]
struct SummaryDto { i: usize, text: String }
#[derive(serde::Deserialize)]
struct EntityDto { kind: String, name: String }
#[derive(serde::Deserialize)]
struct RelationDto { from: String, to: String, #[serde(default)] kind: Option<String> }

/// upsert 实体节点(UNIQUE(kind,name) → 已存在则 last_seen/freq 更新),返回 id。
fn upsert_entity(c: &Connection, kind: &str, name: &str, now: i64) -> rusqlite::Result<i64> {
    c.execute(
        "INSERT INTO entity (kind, name, first_seen, last_seen, freq) VALUES (?1, ?2, ?3, ?3, 1)
         ON CONFLICT(kind, name) DO UPDATE SET last_seen = ?3, freq = freq + 1",
        params![kind, name, now],
    )?;
    c.query_row("SELECT id FROM entity WHERE kind = ?1 AND name = ?2",
        params![kind, name], |r| r.get(0))
}

/// 从 LLM 输出里挖出第一个 JSON 对象(模型常包一段解释)。
fn extract_json(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let end = s.rfind('}')?;
    if end > start { Some(&s[start..=end]) } else { None }
}

/// 空闲巩固:取一批未消化 turn + 当前画像 → LLM 输出更新后的画像 + 偏好 + 逐条摘要。
/// 必须走 ai_queue(AI 串行硬规则)。无未消化 / 禁用 → 直接返回。
pub async fn run_reflection() -> Result<()> {
    if !enabled() {
        return Ok(());
    }
    // 串行闸:已有反思在跑(另一处 spawn)就直接退,避免同批 turn 被消化两次 → 重复 insight。
    let _reflect_guard = match ReflectGuard::try_acquire() {
        Some(g) => g,
        None => return Ok(()),
    };
    let batch = with_db(|c| fetch_unprocessed(c, 12)).unwrap_or_default();
    if batch.is_empty() {
        return Ok(());
    }
    let current_profile = with_db(|c| collect_profile_items(c)).unwrap_or_default();

    // 拼蒸馏 prompt
    let mut turns_txt = String::new();
    for (idx, t) in batch.iter().enumerate() {
        let snippet: String = t.text.chars().take(280).collect();
        turns_txt.push_str(&format!("[{}] {}: {}\n", idx + 1, t.role, snippet));
    }
    let profile_txt = if current_profile.is_empty() {
        "（暂无）".to_string()
    } else {
        current_profile.iter().map(|p| format!("- {}", p.text)).collect::<Vec<_>>().join("\n")
    };
    let prompt = format!(
        "你在维护一个桌面助手对用户的【长期记忆画像】。下面是你目前对用户的认识,以及最近\
         还没消化的几轮交互。请输出**更新后**的画像。\n\n\
         目前的画像:\n{profile_txt}\n\n最近交互:\n{turns_txt}\n\n\
         只输出一个 JSON 对象(不要解释、不要 markdown 围栏),字段:\n\
         - \"insights\": 数组,每项 {{\"kind\":\"profile|pattern|project_state\",\"text\":\"一句话\",\"confidence\":0~1}}\n\
           (这是更新后的**完整**画像,涵盖用户的技术栈/沟通偏好/在做的项目/常用工具等,旧的若仍成立请保留)\n\
         - \"preferences\": 数组,每项 {{\"key\":\"answer_length|tech_stack|tone|language|...\",\"value\":\"...\",\"confidence\":0~1}}\n\
         - \"summaries\": 数组,每项 {{\"i\":交互序号,\"text\":\"这轮一句话摘要\"}}\n\
         - \"entities\": 数组,每项 {{\"kind\":\"project|app|file|topic|tool|person\",\"name\":\"实体名\"}}\n\
           (从这几轮里出现的项目/应用/文件/话题/工具/人,用于构建知识图谱节点)\n\
         - \"relations\": 数组,每项 {{\"from\":\"实体名\",\"to\":\"实体名\",\"kind\":\"about|co_occurs\"}}\n\
           (实体之间的关系,比如 话题 about 项目、两个工具 co_occurs)\n\
         画像精炼克制,最多 8 条 insight、8 条 preference、12 个 entity。"
    );

    let backend = crate::config::Config::load().backend;
    let _ticket = crate::ai_queue::acquire().await; // 串行 + 忙碌可见
    let raw = match crate::backend::ask_text_only(backend, &prompt).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[mouseclaw] reflection LLM failed: {e:#} —— 本批保持未消化,下次再试");
            return Ok(());
        }
    };
    drop(_ticket);

    let parsed: Extraction = match extract_json(&raw).and_then(|j| serde_json::from_str(j).ok()) {
        Some(p) => p,
        None => {
            eprintln!("[mouseclaw] reflection 解析 JSON 失败,跳过(原文前 120 字: {})",
                raw.chars().take(120).collect::<String>());
            // 仍标记 processed,避免坏批次反复重试卡住队列
            let ids: Vec<i64> = batch.iter().map(|t| t.id).collect();
            let _ = with_db(|c| { mark_processed(c, &ids) });
            return Ok(());
        }
    };

    let now = Utc::now().timestamp();
    let res = with_db(|c| {
        // 画像 = 最新一次反思的完整输出:把旧的有效 insight/preference 失效,插入新的
        if !parsed.insights.is_empty() {
            c.execute("UPDATE insight SET valid_to = ?1 WHERE valid_to IS NULL", params![now])?;
            for ins in &parsed.insights {
                c.execute(
                    "INSERT INTO insight (kind, text, confidence, valid_from, valid_to, updated)
                     VALUES (?1, ?2, ?3, ?4, NULL, ?4)",
                    params![ins.kind, ins.text, ins.confidence.unwrap_or(0.6), now],
                )?;
            }
        }
        for p in &parsed.preferences {
            c.execute("UPDATE preference SET valid_to = ?1 WHERE key = ?2 AND valid_to IS NULL",
                params![now, p.key])?;
            c.execute(
                "INSERT INTO preference (key, value, confidence, valid_from, valid_to)
                 VALUES (?1, ?2, ?3, ?4, NULL)",
                params![p.key, p.value, p.confidence.unwrap_or(0.6), now],
            )?;
        }
        for s in &parsed.summaries {
            if let Some(t) = batch.get(s.i.saturating_sub(1)) {
                c.execute("UPDATE memory_turn SET summary = ?1 WHERE id = ?2",
                    params![s.text, t.id])?;
            }
        }
        // 知识图谱:实体节点 + 边
        let mut name_to_id: HashMap<String, i64> = HashMap::new();
        for e in &parsed.entities {
            let name = e.name.trim();
            if name.is_empty() { continue; }
            let id = upsert_entity(c, e.kind.trim(), name, now)?;
            name_to_id.insert(name.to_lowercase(), id);
        }
        // turn → entity 提及边(确定性子串匹配,不额外烧 LLM)。去重防膨胀。
        for t in &batch {
            let tl = t.text.to_lowercase();
            for (nl, eid) in &name_to_id {
                if nl.chars().count() >= 2 && tl.contains(nl.as_str()) {
                    c.execute(
                        "INSERT INTO edge (src, dst, kind, weight, ts)
                         SELECT ?1, ?2, 'mentions', 1, ?3
                         WHERE NOT EXISTS (SELECT 1 FROM edge WHERE src=?1 AND dst=?2 AND kind='mentions')",
                        params![t.id, eid, now],
                    )?;
                }
            }
        }
        // entity ↔ entity 关系边(去重)
        for r in &parsed.relations {
            let a = name_to_id.get(r.from.trim().to_lowercase().as_str()).copied();
            let b = name_to_id.get(r.to.trim().to_lowercase().as_str()).copied();
            if let (Some(a), Some(b)) = (a, b) {
                if a != b {
                    let k = r.kind.as_deref().unwrap_or("co_occurs");
                    c.execute(
                        "INSERT INTO edge (src, dst, kind, weight, ts)
                         SELECT ?1, ?2, ?3, 1, ?4
                         WHERE NOT EXISTS (SELECT 1 FROM edge WHERE src=?1 AND dst=?2 AND kind=?3)",
                        params![a, b, k, now],
                    )?;
                }
            }
        }
        let ids: Vec<i64> = batch.iter().map(|t| t.id).collect();
        mark_processed(c, &ids)?;
        Ok(())
    });
    if let Err(e) = res {
        eprintln!("[mouseclaw] reflection 写库失败: {e:#}");
    } else {
        println!("[mouseclaw] reflection ✓ 消化 {} 轮 → {} insight / {} preference / {} entity",
            batch.len(), parsed.insights.len(), parsed.preferences.len(), parsed.entities.len());
    }
    Ok(())
}

fn mark_processed(c: &Connection, ids: &[i64]) -> rusqlite::Result<()> {
    for id in ids {
        c.execute("UPDATE memory_turn SET processed = 1 WHERE id = ?1", params![id])?;
    }
    Ok(())
}

// ── Tauri 命令(P3 记忆查看器用) ─────────────────────────────────────

/// 历史 turn 列表(给「🕑 历史」tab)。可选关键词过滤(子串)。
#[tauri::command]
pub fn memory_list_turns(query: Option<String>) -> Vec<serde_json::Value> {
    let q = query.unwrap_or_default();
    let ql = q.trim().to_lowercase();
    with_db(|c| {
        let mut st = c.prepare(
            "SELECT id, ts, app, role, text, summary, importance FROM memory_turn
             ORDER BY ts DESC LIMIT 200",
        )?;
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, i64>(6)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, ts, app, role, text, summary, importance) = row?;
            let hay = format!("{} {}", summary.clone().unwrap_or_default(), text).to_lowercase();
            if !ql.is_empty() && !hay.contains(&ql) {
                continue;
            }
            out.push(serde_json::json!({
                "id": id, "ts": ts, "app": app, "role": role,
                "summary": summary.unwrap_or_else(|| text.chars().take(80).collect()),
                "importance": importance,
            }));
        }
        Ok(out)
    })
    .unwrap_or_default()
}

/// 当前画像(给「📌 关于你」tab):insight + preference。
#[tauri::command]
pub fn memory_get_profile() -> serde_json::Value {
    with_db(|c| {
        let mut si = c.prepare(
            "SELECT id, kind, text, confidence FROM insight WHERE valid_to IS NULL
             ORDER BY confidence DESC",
        )?;
        let irows = si.query_map([], |r| Ok(serde_json::json!({
            "id": r.get::<_, i64>(0)?, "kind": r.get::<_, String>(1)?,
            "text": r.get::<_, String>(2)?, "confidence": r.get::<_, f64>(3)?,
        })))?;
        let mut insights = Vec::new();
        for r in irows { insights.push(r?); }
        let mut sp = c.prepare(
            "SELECT id, key, value, confidence FROM preference WHERE valid_to IS NULL
             ORDER BY confidence DESC",
        )?;
        let prows = sp.query_map([], |r| Ok(serde_json::json!({
            "id": r.get::<_, i64>(0)?, "key": r.get::<_, String>(1)?,
            "value": r.get::<_, String>(2)?, "confidence": r.get::<_, f64>(3)?,
        })))?;
        let mut prefs = Vec::new();
        for r in prows { prefs.push(r?); }
        Ok(serde_json::json!({ "insights": insights, "preferences": prefs }))
    })
    .unwrap_or_else(|_| serde_json::json!({ "insights": [], "preferences": [] }))
}

/// 知识图谱(给「🕸 关系」tab):实体节点 + entity↔entity 边。
#[tauri::command]
pub fn memory_get_graph() -> serde_json::Value {
    with_db(|c| {
        let mut es = c.prepare(
            "SELECT id, kind, name, freq FROM entity ORDER BY freq DESC, last_seen DESC LIMIT 60",
        )?;
        let erows = es.query_map([], |r| Ok(serde_json::json!({
            "id": r.get::<_, i64>(0)?, "kind": r.get::<_, String>(1)?,
            "name": r.get::<_, String>(2)?, "freq": r.get::<_, i64>(3)?,
        })))?;
        let mut nodes = Vec::new();
        for r in erows { nodes.push(r?); }
        let mut xs = c.prepare(
            "SELECT src, dst, kind FROM edge WHERE kind IN ('co_occurs','about') LIMIT 300",
        )?;
        let xrows = xs.query_map([], |r| Ok(serde_json::json!({
            "src": r.get::<_, i64>(0)?, "dst": r.get::<_, i64>(1)?, "kind": r.get::<_, String>(2)?,
        })))?;
        let mut edges = Vec::new();
        for r in xrows { edges.push(r?); }
        Ok(serde_json::json!({ "nodes": nodes, "edges": edges }))
    })
    .unwrap_or_else(|_| serde_json::json!({ "nodes": [], "edges": [] }))
}

#[tauri::command]
pub fn memory_delete_turn(id: i64) -> Result<(), String> {
    with_db(|c| { c.execute("DELETE FROM memory_turn WHERE id = ?1", params![id])?; Ok(()) })
        .map_err(|e| e.to_string())
}

/// 删一条画像:insight(kind=insight) 或 preference(kind=preference)。
#[tauri::command]
pub fn memory_delete_profile_item(kind: String, id: i64) -> Result<(), String> {
    let table = if kind == "preference" { "preference" } else { "insight" };
    with_db(|c| {
        c.execute(&format!("DELETE FROM {table} WHERE id = ?1"), params![id])?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn memory_clear_all() -> Result<(), String> {
    with_db(|c| {
        c.execute_batch(
            "DELETE FROM memory_turn; DELETE FROM entity; DELETE FROM edge;
             DELETE FROM preference; DELETE FROM insight;",
        )?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}

/// 记忆设置(给查看器顶部开关)。
#[tauri::command]
pub fn memory_get_settings() -> serde_json::Value {
    let cfg = crate::config::Config::load();
    serde_json::json!({ "enabled": cfg.memory_enabled, "paused": cfg.memory_paused })
}

/// 暂停 = ChatGPT 的 Temporary:既不读也不写记忆。
#[tauri::command]
pub fn memory_set_paused(paused: bool) -> Result<(), String> {
    let mut cfg = crate::config::Config::load();
    cfg.memory_paused = paused;
    cfg.save().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn memory_set_enabled(enabled: bool) -> Result<(), String> {
    let mut cfg = crate::config::Config::load();
    cfg.memory_enabled = enabled;
    cfg.save().map_err(|e| e.to_string())
}
