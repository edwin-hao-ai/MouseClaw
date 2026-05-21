//! 长期记忆 · 本地 SQLite(无向量库) · 三层(画像 / 情景 / 图谱)
//!
//! 设计:docs/design/pet-identity-and-memory-20260521.md
//!
//! 取舍(为什么这么写):
//!   - **无向量模型**:检索靠 关系表 + 时近/重要度/token 重叠 打分,在 Rust 内完成,
//!     零额外常驻内存(守 600MB 目标)。FTS5/BM25 升级留到能在 Mac 上跑测之后(v1.1)。
//!   - **记录零额外 LLM**:`record_turn` 只写库。把 LLM 蒸馏挪到空闲 `run_reflection`
//!     批处理(P2.5),省 token、记录路径不卡。
//!   - **backend 无关**:reflection 走 `backend::ask_text_only`(4 后端 + 未来直连 LLM 通吃);
//!     注入走 `claude_cli::build_prompt`,所有 backend 自动生效。
//!   - **隐私**:只存"用户召唤那一刻"已有的素材(app 标题 / 文本 / 截图路径),不做后台监控。
//!     纯本地 `~/.mouseclaw/memory.db`。`memory_enabled` / `memory_paused` 双开关。

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use once_cell::sync::Lazy;
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::Mutex;

/// 单连接 + Mutex 串行访问(记忆读写量小,SELECT < 10ms)。Connection 是 Send。
static MEM: Lazy<Mutex<Option<Connection>>> = Lazy::new(|| Mutex::new(None));

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
            || matches!(c, '，' | '。' | '、' | '？' | '！' | '：' | '；' | '"' | '"' | '（' | '）'))
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

struct ScoredTurn { ts: i64, app: Option<String>, snippet: String, score: f64 }

/// 当前画像(always-on):有效 insight + 有效 preference。
fn collect_profile(c: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut out = Vec::new();
    let mut st = c.prepare(
        "SELECT text FROM insight WHERE valid_to IS NULL AND kind IN ('profile','pattern')
         ORDER BY confidence DESC LIMIT 6",
    )?;
    let rows = st.query_map([], |r| r.get::<_, String>(0))?;
    for r in rows { out.push(r?); }
    let mut sp = c.prepare(
        "SELECT key, value FROM preference WHERE valid_to IS NULL ORDER BY confidence DESC LIMIT 6",
    )?;
    let prefs = sp.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for p in prefs { let (k, v) = p?; out.push(format!("{k}:{v}")); }
    Ok(out)
}

/// 取最近 N 条 turn,在 Rust 内按 token 重叠 + 同 app + 时近 + 重要度 打分,选 top。
fn collect_relevant(c: &Connection, query: &str, app: Option<&str>, limit: usize)
    -> rusqlite::Result<Vec<ScoredTurn>>
{
    let toks = query_tokens(query);
    let now = Utc::now().timestamp();
    let mut st = c.prepare(
        "SELECT ts, app, role, text, summary, importance FROM memory_turn
         ORDER BY ts DESC LIMIT 120",
    )?;
    let rows = st.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, Option<String>>(4)?,
            r.get::<_, i64>(5)?,
        ))
    })?;
    let mut scored: Vec<ScoredTurn> = Vec::new();
    for row in rows {
        let (ts, tapp, _role, text, summary, importance) = row?;
        let hay = summary.clone().unwrap_or_else(|| text.clone());
        let hay_l = hay.to_lowercase();
        // relevance:命中的 token 数
        let hits = toks.iter().filter(|t| hay_l.contains(t.to_lowercase().as_str())).count();
        let mut score = hits as f64 * 3.0;
        // 同 app 加权
        if let (Some(a), Some(b)) = (app, tapp.as_deref()) {
            if a == b { score += 1.5; }
        }
        // 时近(指数衰减,7 天半衰期)
        let age_days = ((now - ts).max(0) as f64) / 86400.0;
        score += 2.0 * 0.5_f64.powf(age_days / 7.0);
        // 重要度
        score += importance as f64 * 0.4;
        if hits == 0 && score < 2.6 {
            continue; // 既不相关又不够近/重要 → 丢
        }
        let snippet = summary.unwrap_or_else(|| {
            let s: String = text.chars().take(60).collect();
            if text.chars().count() > 60 { format!("{s}…") } else { s }
        });
        scored.push(ScoredTurn { ts, app: tapp, snippet, score });
    }
    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    Ok(scored)
}

/// 注入 prompt 的记忆块。无内容 / 禁用 / 暂停 → None。
pub fn retrieve_block(query: &str, app: Option<&str>) -> Option<String> {
    if !enabled() {
        return None;
    }
    let res: Result<Option<String>> = with_db(|c| {
        let profile = collect_profile(c).unwrap_or_default();
        let turns = collect_relevant(c, query, app, 5).unwrap_or_default();
        if profile.is_empty() && turns.is_empty() {
            return Ok(None);
        }
        let mut s = String::from("[记忆 · 仅供参考,以当前任务为准]\n");
        for p in &profile {
            s.push_str("- ");
            s.push_str(p);
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

#[derive(serde::Deserialize, Default)]
struct Extraction {
    #[serde(default)] insights: Vec<InsightDto>,
    #[serde(default)] preferences: Vec<PrefDto>,
    #[serde(default)] summaries: Vec<SummaryDto>,
}
#[derive(serde::Deserialize)]
struct InsightDto { kind: String, text: String, #[serde(default)] confidence: Option<f64> }
#[derive(serde::Deserialize)]
struct PrefDto { key: String, value: String, #[serde(default)] confidence: Option<f64> }
#[derive(serde::Deserialize)]
struct SummaryDto { i: usize, text: String }

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
    let batch = with_db(|c| fetch_unprocessed(c, 12)).unwrap_or_default();
    if batch.is_empty() {
        return Ok(());
    }
    let current_profile = with_db(|c| collect_profile(c)).unwrap_or_default();

    // 拼蒸馏 prompt
    let mut turns_txt = String::new();
    for (idx, t) in batch.iter().enumerate() {
        let snippet: String = t.text.chars().take(280).collect();
        turns_txt.push_str(&format!("[{}] {}: {}\n", idx + 1, t.role, snippet));
    }
    let profile_txt = if current_profile.is_empty() {
        "（暂无）".to_string()
    } else {
        current_profile.iter().map(|p| format!("- {p}")).collect::<Vec<_>>().join("\n")
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
         画像精炼克制,最多 8 条 insight、8 条 preference。"
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
        let ids: Vec<i64> = batch.iter().map(|t| t.id).collect();
        mark_processed(c, &ids)?;
        Ok(())
    });
    if let Err(e) = res {
        eprintln!("[mouseclaw] reflection 写库失败: {e:#}");
    } else {
        println!("[mouseclaw] reflection ✓ 消化 {} 轮 → {} insight / {} preference",
            batch.len(), parsed.insights.len(), parsed.preferences.len());
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
