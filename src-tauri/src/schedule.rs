//! 定时任务 / 心跳（v0.5）—— 数据模型 + 持久化。
//!
//! 用户一句话（语音 / 任务窗）→ backend 解析成结构化 `ScheduleTask` → 存
//! `~/.mouseclaw/schedules.json`。每次执行的完整结果 append 到
//! `~/.mouseclaw/schedule_runs.jsonl`（仿 sessions.jsonl）。
//!
//! 调度循环在 `scheduler.rs`（复刻 nudge.rs 的 60s tokio interval）。
//! 安全红线（见 CLAUDE.md / prototype）：定时任务永远走 A 模式纯文本、永远不截屏。
//!
//! 并发：commands（创建/删除/开关）和 scheduler tick 都会读改 schedules.json。
//! 用一把 `FILE_LOCK` 串行所有 read-modify-write —— 闭包内只做同步文件 IO，
//! **绝不**在持锁期间 `.await`（AI 调用在锁外）。

use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Duration as ChronoDuration, Local, NaiveDate, NaiveTime, TimeZone, Utc, Weekday};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

/// 错过多久之内的定时还补跑（超过就标记"错过"跳过，避免跑过期内容 / 一次堆一堆）。
pub const CATCHUP_GRACE_SECS: i64 = 6 * 60 * 60;

static FILE_LOCK: Lazy<std::sync::Mutex<()>> = Lazy::new(|| std::sync::Mutex::new(()));

// ──────────────────────────────────────────────────────────────────
// 数据模型
// ──────────────────────────────────────────────────────────────────

/// 定时方式（kind 五选一）。serde camelCase 字段跟前端 TS 对齐。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Schedule {
    /// 每天 HH:MM
    Daily { time: String },
    /// 工作日（周一到周五）HH:MM
    Weekday { time: String },
    /// 每周指定几天（1=周一 .. 7=周日）HH:MM
    Weekly {
        days: Vec<u32>,
        time: String,
    },
    /// 每隔 N 分钟（心跳）。可选活跃时段（默认全天）—— 半夜不打扰。
    Interval {
        #[serde(rename = "everyMinutes")]
        every_minutes: u32,
        #[serde(rename = "activeStart", default, skip_serializing_if = "Option::is_none")]
        active_start: Option<String>,
        #[serde(rename = "activeEnd", default, skip_serializing_if = "Option::is_none")]
        active_end: Option<String>,
    },
    /// 每月某天（1-31，超出当月天数自动 clamp 到月末）HH:MM
    Monthly { day: u32, time: String },
}

/// 一次执行的状态。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Ok,
    Failed,
    Skipped,
}

/// 任务上一次执行的摘要（存进 task，给列表显示"上次✓/✗"）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunInfo {
    pub at: DateTime<Utc>,
    pub status: RunStatus,
    pub summary: String,
}

/// 一个定时任务（持久化在 schedules.json）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleTask {
    pub id: String,
    pub title: String,
    pub action: String,
    pub schedule: Schedule,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "lastRun", default, skip_serializing_if = "Option::is_none")]
    pub last_run: Option<RunInfo>,
}

fn default_true() -> bool {
    true
}

/// 前端 / backend 解析出来的"新建任务"输入（无 id / createdAt / lastRun）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleInput {
    pub title: String,
    pub action: String,
    pub schedule: Schedule,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// 给前端列表用的视图 —— task 全字段 + 计算出的 nextRun（本地 RFC3339）。
#[derive(Debug, Clone, Serialize)]
pub struct ScheduleView {
    #[serde(flatten)]
    pub task: ScheduleTask,
    #[serde(rename = "nextRun", skip_serializing_if = "Option::is_none")]
    pub next_run: Option<String>,
}

/// schedule_runs.jsonl 里的一行 —— 含完整 AI 输出（任务窗"展开"看全文）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    #[serde(rename = "taskId")]
    pub task_id: String,
    pub title: String,
    pub at: DateTime<Utc>,
    pub status: RunStatus,
    pub summary: String,
    #[serde(default)]
    pub output: String,
}

// ──────────────────────────────────────────────────────────────────
// 下次执行时间计算（pure，单测覆盖）
// ──────────────────────────────────────────────────────────────────

fn parse_hm(s: &str) -> Option<(u32, u32)> {
    let (h, m) = s.split_once(':')?;
    let h: u32 = h.trim().parse().ok()?;
    let m: u32 = m.trim().parse().ok()?;
    if h < 24 && m < 60 {
        Some((h, m))
    } else {
        None
    }
}

fn weekday_num(wd: Weekday) -> u32 {
    wd.number_from_monday() // 1=Mon .. 7=Sun
}

/// 构造本地某天某时刻的 DateTime<Local>，处理 DST 折叠（取最早的有效解）。
fn local_dt(date: NaiveDate, h: u32, m: u32) -> Option<DateTime<Local>> {
    let t = NaiveTime::from_hms_opt(h, m, 0)?;
    let ndt = date.and_time(t);
    Local
        .from_local_datetime(&ndt)
        .earliest()
        .or_else(|| Local.from_local_datetime(&ndt).latest())
}

/// 找到 `from` 之后第一个满足 `day_ok(weekday)` 的 HH:MM。
fn next_daily(
    from: DateTime<Local>,
    h: u32,
    m: u32,
    day_ok: &dyn Fn(Weekday) -> bool,
) -> Option<DateTime<Local>> {
    let base = from.date_naive();
    for off in 0..=370i64 {
        let date = base + ChronoDuration::days(off);
        if !day_ok(date.weekday()) {
            continue;
        }
        if let Some(dt) = local_dt(date, h, m) {
            if dt > from {
                return Some(dt);
            }
        }
    }
    None
}

fn last_day_of_month(year: i32, month: u32) -> u32 {
    // 下个月 1 号往前推一天
    let (ny, nm) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    let first_next = NaiveDate::from_ymd_opt(ny, nm, 1).unwrap();
    (first_next - ChronoDuration::days(1)).day()
}

fn next_monthly(from: DateTime<Local>, day: u32, h: u32, m: u32) -> Option<DateTime<Local>> {
    let mut year = from.year();
    let mut month = from.month();
    for _ in 0..=24 {
        let dom = day.min(last_day_of_month(year, month)).max(1);
        if let Some(date) = NaiveDate::from_ymd_opt(year, month, dom) {
            if let Some(dt) = local_dt(date, h, m) {
                if dt > from {
                    return Some(dt);
                }
            }
        }
        if month == 12 {
            year += 1;
            month = 1;
        } else {
            month += 1;
        }
    }
    None
}

fn next_interval(
    from: DateTime<Local>,
    every_minutes: u32,
    active_start: Option<&str>,
    active_end: Option<&str>,
) -> Option<DateTime<Local>> {
    let n = every_minutes.max(1) as i64;
    let cand = from + ChronoDuration::minutes(n);
    let (Some(s), Some(e)) = (active_start, active_end) else {
        return Some(cand); // 无活跃时段 = 全天
    };
    let (sh, sm) = parse_hm(s)?;
    let (eh, em) = parse_hm(e)?;
    let start_t = NaiveTime::from_hms_opt(sh, sm, 0)?;
    let end_t = NaiveTime::from_hms_opt(eh, em, 0)?;
    let t = cand.time();
    if t < start_t {
        // 太早 → 拉到当天活跃开始
        local_dt(cand.date_naive(), sh, sm)
    } else if t > end_t {
        // 过了活跃结束 → 明天活跃开始
        local_dt(cand.date_naive() + ChronoDuration::days(1), sh, sm)
    } else {
        Some(cand)
    }
}

impl Schedule {
    /// `from` 之后的下一次触发时间。None = 无法计算（time 格式坏等）。
    pub fn next_after(&self, from: DateTime<Local>) -> Option<DateTime<Local>> {
        match self {
            Schedule::Daily { time } => {
                let (h, m) = parse_hm(time)?;
                next_daily(from, h, m, &|_| true)
            }
            Schedule::Weekday { time } => {
                let (h, m) = parse_hm(time)?;
                next_daily(from, h, m, &|wd| {
                    !matches!(wd, Weekday::Sat | Weekday::Sun)
                })
            }
            Schedule::Weekly { days, time } => {
                let (h, m) = parse_hm(time)?;
                let set: HashSet<u32> = days.iter().copied().collect();
                if set.is_empty() {
                    return None;
                }
                next_daily(from, h, m, &|wd| set.contains(&weekday_num(wd)))
            }
            Schedule::Monthly { day, time } => {
                let (h, m) = parse_hm(time)?;
                next_monthly(from, *day, h, m)
            }
            Schedule::Interval {
                every_minutes,
                active_start,
                active_end,
            } => next_interval(
                from,
                *every_minutes,
                active_start.as_deref(),
                active_end.as_deref(),
            ),
        }
    }
}

impl ScheduleTask {
    /// 计算下一次触发（从 last_run 或 created_at 起算）。
    pub fn next_after_last(&self, now: DateTime<Local>) -> Option<DateTime<Local>> {
        let from = self
            .last_run
            .as_ref()
            .map(|r| r.at.with_timezone(&Local))
            .unwrap_or_else(|| self.created_at.with_timezone(&Local));
        // from 不能在未来（last_run 理论上 <= now），保险起见取 min。
        let from = from.min(now);
        self.schedule.next_after(from)
    }
}

// ──────────────────────────────────────────────────────────────────
// 持久化
// ──────────────────────────────────────────────────────────────────

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn schedules_path() -> Option<PathBuf> {
    home().map(|h| h.join(".mouseclaw/schedules.json"))
}

fn runs_path() -> Option<PathBuf> {
    home().map(|h| h.join(".mouseclaw/schedule_runs.jsonl"))
}

fn load_unlocked() -> Vec<ScheduleTask> {
    let Some(path) = schedules_path() else {
        return Vec::new();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return Vec::new();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn save_unlocked(tasks: &[ScheduleTask]) -> Result<()> {
    let path = schedules_path().context("HOME not set")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let json = serde_json::to_vec_pretty(tasks)?;
    std::fs::write(&path, json).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// 串行的 read-modify-write —— 所有改 schedules.json 的路径都走这里。
/// ⚠️ 闭包里**不要** `.await`（持的是 std::sync::Mutex）。
pub fn with_tasks<R>(f: impl FnOnce(&mut Vec<ScheduleTask>) -> R) -> R {
    let _g = FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut tasks = load_unlocked();
    let r = f(&mut tasks);
    if let Err(e) = save_unlocked(&tasks) {
        eprintln!("[mouseclaw] schedule save failed: {e}");
    }
    r
}

/// 只读取（不改）—— list 用。
pub fn load() -> Vec<ScheduleTask> {
    let _g = FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    load_unlocked()
}

fn gen_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("t{nanos:x}")
}

/// 新建任务（去重 id），返回创建好的 task。
pub fn create(input: ScheduleInput) -> ScheduleTask {
    let task = ScheduleTask {
        id: gen_id(),
        title: input.title.trim().to_string(),
        action: input.action.trim().to_string(),
        schedule: input.schedule,
        enabled: input.enabled,
        created_at: Utc::now(),
        last_run: None,
    };
    let t2 = task.clone();
    with_tasks(|tasks| tasks.push(t2));
    task
}

/// 更新已有任务的可编辑字段（title / action / schedule）。返回是否找到。
pub fn update(id: &str, input: ScheduleInput) -> bool {
    with_tasks(|tasks| {
        if let Some(t) = tasks.iter_mut().find(|t| t.id == id) {
            t.title = input.title.trim().to_string();
            t.action = input.action.trim().to_string();
            t.schedule = input.schedule;
            t.enabled = input.enabled;
            true
        } else {
            false
        }
    })
}

pub fn delete(id: &str) -> bool {
    with_tasks(|tasks| {
        let before = tasks.len();
        tasks.retain(|t| t.id != id);
        tasks.len() != before
    })
}

pub fn set_enabled(id: &str, enabled: bool) -> bool {
    with_tasks(|tasks| {
        if let Some(t) = tasks.iter_mut().find(|t| t.id == id) {
            t.enabled = enabled;
            // 重新启用时把 last_run 拨到现在，避免立刻补跑一堆暂停期间错过的 slot
            if enabled {
                if let Some(r) = t.last_run.as_mut() {
                    r.at = Utc::now();
                }
            }
            true
        } else {
            false
        }
    })
}

/// 写回某任务的 last_run（scheduler tick 用）。task 可能已被用户删 → 找不到就忽略。
pub fn set_last_run(id: &str, info: RunInfo) {
    with_tasks(|tasks| {
        if let Some(t) = tasks.iter_mut().find(|t| t.id == id) {
            t.last_run = Some(info);
        }
    });
}

/// append 一条执行记录到 jsonl。⚠️ 不要在 with_tasks 闭包内调（同锁会死锁）。
pub fn append_run(rec: &RunRecord) {
    let _g = FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Some(path) = runs_path() else { return };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let Ok(mut line) = serde_json::to_string(rec) else { return };
    line.push('\n');
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// 读某任务最近 N 条执行记录（任务窗"展开看历史"）。最新在前。
pub fn recent_runs(task_id: &str, n: usize) -> Vec<RunRecord> {
    let _g = FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Some(path) = runs_path() else {
        return Vec::new();
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let mut out: Vec<RunRecord> = content
        .lines()
        .filter_map(|l| serde_json::from_str::<RunRecord>(l).ok())
        .filter(|r| r.task_id == task_id)
        .collect();
    out.reverse();
    out.truncate(n);
    out
}

/// 解析 backend 吐出的 `[SCHEDULE]` JSON / 任务窗一句话解析结果。
pub fn parse_input(json: &str) -> Result<ScheduleInput> {
    let mut input: ScheduleInput =
        serde_json::from_str(json.trim()).context("解析 schedule JSON 失败")?;
    if input.title.trim().is_empty() {
        input.title = "定时任务".to_string();
    }
    anyhow::ensure!(!input.action.trim().is_empty(), "action 不能为空");
    Ok(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike; // .hour() in assertions

    fn dt(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Local> {
        Local
            .from_local_datetime(&NaiveDate::from_ymd_opt(y, mo, d).unwrap().and_hms_opt(h, mi, 0).unwrap())
            .unwrap()
    }

    #[test]
    fn parse_hm_works() {
        assert_eq!(parse_hm("08:00"), Some((8, 0)));
        assert_eq!(parse_hm("23:59"), Some((23, 59)));
        assert_eq!(parse_hm("24:00"), None);
        assert_eq!(parse_hm("8"), None);
    }

    #[test]
    fn daily_next_is_today_if_before_else_tomorrow() {
        let s = Schedule::Daily { time: "08:00".into() };
        // 创建于 07:00 → 今天 08:00
        let n = s.next_after(dt(2026, 5, 22, 7, 0)).unwrap();
        assert_eq!((n.day(), n.hour()), (22, 8));
        // 创建于 09:00 → 明天 08:00
        let n = s.next_after(dt(2026, 5, 22, 9, 0)).unwrap();
        assert_eq!((n.day(), n.hour()), (23, 8));
    }

    #[test]
    fn weekday_skips_weekend() {
        // 2026-05-22 是周五。周五 18:00 之后 → 下一个工作日 = 周一 25 号
        let s = Schedule::Weekday { time: "09:00".into() };
        let n = s.next_after(dt(2026, 5, 22, 19, 0)).unwrap();
        assert_eq!(n.weekday(), Weekday::Mon);
        assert_eq!(n.day(), 25);
    }

    #[test]
    fn weekly_picks_listed_days() {
        // 只在周一(1)、周三(3) 跑。从周五 22 号 → 下周一 25 号
        let s = Schedule::Weekly { days: vec![1, 3], time: "09:00".into() };
        let n = s.next_after(dt(2026, 5, 22, 19, 0)).unwrap();
        assert_eq!(n.weekday(), Weekday::Mon);
    }

    #[test]
    fn interval_adds_minutes_within_window() {
        let s = Schedule::Interval {
            every_minutes: 120,
            active_start: Some("09:00".into()),
            active_end: Some("22:00".into()),
        };
        // 10:00 + 2h = 12:00（在窗内）
        let n = s.next_after(dt(2026, 5, 22, 10, 0)).unwrap();
        assert_eq!(n.hour(), 12);
        // 21:00 + 2h = 23:00（超出 22:00）→ 明天 09:00
        let n = s.next_after(dt(2026, 5, 22, 21, 0)).unwrap();
        assert_eq!((n.day(), n.hour()), (23, 9));
    }

    #[test]
    fn monthly_clamps_to_month_end() {
        // 每月 31 号；从 2 月初 → 2 月没有 31 → clamp 到 28（2026 非闰年）
        let s = Schedule::Monthly { day: 31, time: "10:00".into() };
        let n = s.next_after(dt(2026, 2, 1, 0, 0)).unwrap();
        assert_eq!((n.month(), n.day()), (2, 28));
    }

    #[test]
    fn serde_roundtrip_kinds() {
        let s = Schedule::Daily { time: "08:00".into() };
        let j = serde_json::to_string(&s).unwrap();
        assert!(j.contains("\"kind\":\"daily\""));
        let back: Schedule = serde_json::from_str(&j).unwrap();
        assert_eq!(s, back);

        let iv: Schedule = serde_json::from_str(
            r#"{"kind":"interval","everyMinutes":120,"activeStart":"09:00","activeEnd":"22:00"}"#,
        )
        .unwrap();
        assert!(matches!(iv, Schedule::Interval { every_minutes: 120, .. }));
    }

    #[test]
    fn parse_input_fills_title_validates_action() {
        let ok = parse_input(r#"{"title":"","action":"收集AI新闻","schedule":{"kind":"daily","time":"08:00"}}"#).unwrap();
        assert_eq!(ok.title, "定时任务");
        assert!(parse_input(r#"{"title":"x","action":"","schedule":{"kind":"daily","time":"08:00"}}"#).is_err());
    }
}
