//! 定时任务调度循环（v0.5）—— 复刻 nudge.rs 的 60s tokio interval。
//!
//! 每分钟扫 schedules.json，找到点的任务：
//!   1. 先把 last_run 拨到现在（占住这个 slot，防 AI 跑很久期间被重复触发）
//!   2. 锁外跑 `ai_queue::acquire()` + `backend::ask_text_only`（串行 + 忙碌 badge）
//!   3. 回写真实结果 + append 到 schedule_runs.jsonl + emit 轻气泡（仅成功）
//!
//! 安全红线：纯文本（无截图）、A 模式（结果不写光标）、失败安静记录不弹刺眼气泡、
//! Mac 睡眠错过的 slot 合并成一次（超宽限窗就跳过不跑过期内容）。

use std::time::Duration;

use chrono::{Local, Utc};
use tauri::{AppHandle, Emitter};
use tokio::time::sleep;

use crate::events::{ScheduleResultPayload, EV_SCHEDULE_RESULT};
use crate::schedule::{self, RunInfo, RunRecord, RunStatus};

const TICK_SECS: u64 = 60;
/// 启动后等一会再开始（让 app / 网络稳定，错开开机其它初始化）。
const STARTUP_DELAY_SECS: u64 = 30;
/// 一次性"发现提示"延迟 —— 启动 ~3 分钟后若没建过任何任务才轻轻提一句。
const HINT_DELAY_SECS: u64 = 180;

pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        sleep(Duration::from_secs(STARTUP_DELAY_SECS)).await;
        let mut elapsed = STARTUP_DELAY_SECS;
        let mut hinted = false;
        loop {
            tick(&app).await;
            // 一次性发现提示（onboarded + 从没建过任务 + 没提过）
            if !hinted && elapsed >= HINT_DELAY_SECS {
                hinted = true;
                maybe_hint(&app);
            }
            sleep(Duration::from_secs(TICK_SECS)).await;
            elapsed += TICK_SECS;
        }
    });
    println!("[mouseclaw] 🦞 scheduler spawned ({TICK_SECS}s tick)");
}

/// 扫一遍所有任务，跑到点的。
async fn tick(app: &AppHandle) {
    let cfg = crate::config::Config::load();
    if !cfg.onboarded {
        return; // 还没配 backend
    }
    let now = Local::now();

    // 阶段 1（持锁，无 await）：找出到点的任务 + 立刻占住 slot。
    enum Action {
        Run,
        Skip,
    }
    let due: Vec<(String, String, String, Action)> = schedule::with_tasks(|tasks| {
        let mut v = Vec::new();
        for t in tasks.iter_mut() {
            if !t.enabled {
                continue;
            }
            let Some(slot) = t.next_after_last(now) else {
                continue;
            };
            if slot > now {
                continue; // 还没到
            }
            let stale = (now - slot).num_seconds() > schedule::CATCHUP_GRACE_SECS;
            // 占住 slot：把 last_run 拨到现在（无论跑还是跳，都不重复触发）
            let action = if stale { Action::Skip } else { Action::Run };
            if stale {
                t.last_run = Some(RunInfo {
                    at: Utc::now(),
                    status: RunStatus::Skipped,
                    summary: lang_skip(&cfg.language),
                });
            } else {
                // 临时占位（真实结果在阶段 3 回写）
                t.last_run = Some(RunInfo {
                    at: Utc::now(),
                    status: RunStatus::Ok,
                    summary: lang_running(&cfg.language),
                });
            }
            v.push((t.id.clone(), t.title.clone(), t.action.clone(), action));
        }
        v
    });

    if due.is_empty() {
        return;
    }

    for (id, title, action, what) in due {
        match what {
            Action::Skip => {
                schedule::append_run(&RunRecord {
                    task_id: id.clone(),
                    title: title.clone(),
                    at: Utc::now(),
                    status: RunStatus::Skipped,
                    summary: lang_skip(&cfg.language),
                    output: String::new(),
                });
                println!("[mouseclaw] ⏰ schedule '{title}' 错过太久，跳过");
            }
            Action::Run => {
                run_one(app, &id, &title, &action, cfg.backend, &cfg.language).await;
            }
        }
    }
}

/// 立即手动跑一次某任务（任务窗"▶ 立刻跑"）。找不到 / 已删 → 静默。
pub async fn run_now(app: &AppHandle, id: &str) {
    let cfg = crate::config::Config::load();
    let Some(task) = schedule::load().into_iter().find(|t| t.id == id) else {
        return;
    };
    run_one(app, &task.id, &task.title, &task.action, cfg.backend, &cfg.language).await;
}

/// 跑单个任务（锁外）—— 排队 + 调 backend（失败重试一次）+ 回写 + 通知。
async fn run_one(
    app: &AppHandle,
    id: &str,
    title: &str,
    action: &str,
    backend: crate::backend::Backend,
    lang: &str,
) {
    println!("[mouseclaw] ⏰ running schedule '{title}'");
    let prompt = build_action_prompt(action, lang);

    let result = {
        // 串行队列 + 忙碌 badge（drop 即释放）
        // agentic：放开 WebSearch/WebFetch/Bash 等工具，让 AI 去取真实数据而非编造。
        let _ticket = crate::ai_queue::acquire().await;
        match crate::backend::ask_text_only_agentic(backend, &prompt).await {
            Ok(t) => Ok(t),
            Err(e) => {
                // 自动重试一次（断网 / 5xx 等瞬时故障）
                eprintln!("[mouseclaw] ⏰ '{title}' 第一次失败，5s 后重试一次：{e:#}");
                sleep(Duration::from_secs(5)).await;
                crate::backend::ask_text_only_agentic(backend, &prompt).await
            }
        }
    };

    let (status, summary, output) = match result {
        Ok(text) => {
            let summary = first_line_summary(&text);
            (RunStatus::Ok, summary, text)
        }
        Err(e) => {
            let msg = crate::pipeline::friendly_backend_error(&format!("{e:#}"), backend);
            (RunStatus::Failed, lang_failed(lang), msg)
        }
    };

    // 回写 last_run（任务可能已被用户删 → set_last_run 找不到就忽略）
    schedule::set_last_run(
        id,
        RunInfo {
            at: Utc::now(),
            status,
            summary: summary.clone(),
        },
    );
    schedule::append_run(&RunRecord {
        task_id: id.to_string(),
        title: title.to_string(),
        at: Utc::now(),
        status,
        summary: summary.clone(),
        output,
    });

    // 通知：成功 → 弹桌宠 + 轻气泡；失败 → 不弹桌宠、不弹气泡（见 prototype 红线，
    // App.tsx 按 ok 过滤）。但**无论成功失败都 emit 事件** —— 让任务窗清掉"执行中"态、
    // 刷新出 ✓/✗ 结果（否则手动"立刻跑"失败时前端会一直转圈，用户以为卡死）。
    if status == RunStatus::Ok {
        crate::overlay::show_mouse_at_anchor(app);
    } else {
        println!("[mouseclaw] ⏰ '{title}' 失败，已记进任务窗（不弹气泡打扰）");
    }
    let payload = ScheduleResultPayload {
        task_id: id.to_string(),
        title: title.to_string(),
        summary,
        ok: status == RunStatus::Ok,
    };
    if let Err(e) = app.emit(EV_SCHEDULE_RESULT, &payload) {
        eprintln!("[mouseclaw] schedule-result emit failed: {e}");
    }
}

/// 一次性"发现提示" —— 从没建过任务的老用户，启动几分钟后轻轻提一句怎么用。
fn maybe_hint(app: &AppHandle) {
    let cfg = crate::config::Config::load();
    if !cfg.onboarded || cfg.seen_schedule_hint {
        return;
    }
    if !schedule::load().is_empty() {
        // 已经在用了，不用提
        let mut c = cfg;
        c.seen_schedule_hint = true;
        let _ = c.save();
        return;
    }
    let mut c = cfg;
    c.seen_schedule_hint = true;
    let _ = c.save();

    let summary = if c.language == "en" {
        "Want me to do things on a schedule? Try saying \"summarize AI news every morning\"".to_string()
    } else {
        "想让我定时帮你做事吗？召唤我说一句「每天早上整理 AI 新闻」试试".to_string()
    };
    crate::overlay::show_mouse_at_anchor(app);
    let payload = ScheduleResultPayload {
        task_id: String::new(), // 空 = 提示（前端"展开"打开任务窗）
        title: if c.language == "en" { "Scheduled tasks".into() } else { "定时任务".into() },
        summary,
        ok: true,
    };
    let _ = app.emit(EV_SCHEDULE_RESULT, &payload);
}

/// 把用户的任务指令包成"后台自动执行"的 prompt（无截图、A 模式、可直接读的结果）。
fn build_action_prompt(action: &str, lang: &str) -> String {
    if lang == "en" {
        format!(
            "[MouseClaw scheduled task] This is a background task the user set up earlier and is \
             now running on schedule. There is NO screenshot and NO live conversation — just do \
             what the instruction says and return a result the user can read later. Keep it concise \
             (it shows in a small bubble + a tasks window). Never ask for confirmation, never assume \
             the user is here to interact.\n\n\
             GROUNDING (critical — this runs unattended, the user will trust the output blindly):\n\
             - If the task needs real-time or external info (news, email, weather, prices, web pages, \
               files), you MUST use your tools (WebSearch / WebFetch / Bash / Read) to fetch the \
               REAL data. Do not answer from memory.\n\
             - NEVER fabricate specific facts, numbers, headlines, links, dates or names. If you \
               cannot retrieve something, say so plainly (e.g. \"couldn't fetch X\") — it is far \
               better to report less than to invent.\n\
             - Cite sources / links where possible so the user can verify.\n\n\
             Instruction: {action}\n\nStart with a one-line summary (≤ 12 words, for the bubble), \
             then the detail below it."
        )
    } else {
        format!(
            "【MouseClaw 定时任务】这是用户之前预设、现在到点自动执行的后台任务。\
             没有屏幕截图、没有实时对话——只按下面的指令完成，给出可直接阅读的结果。\
             结果尽量精简（会显示在一个小气泡 + 任务窗口里）。\
             绝不要请求确认，绝不要假设用户正在场互动。\n\n\
             【真实性铁律】（关键——这是无人值守自动跑的，用户会直接信任结果）：\n\
             - 任务若需要实时/外部信息（新闻、邮件、天气、股价、网页、文件等），\
               **必须用你的工具**（WebSearch 搜网 / WebFetch 抓页 / Bash 跑命令 / Read 读文件）\
               去取**真实数据**，不要凭记忆或想象作答。\n\
             - **严禁编造**任何具体事实、数字、标题、链接、日期、人名。取不到就如实说\
               「没能获取到 X」——宁可少说，绝不杜撰。\n\
             - 尽量给出来源/链接，让用户能自行核对。\n\n\
             任务指令：{action}\n\n\
             请第一行用一句话总结（≤20 字，给气泡用），随后再附详细内容。"
        )
    }
}

/// 取首行非空文本当摘要（给气泡 / 列表"上次"用），截断防过长。
fn first_line_summary(text: &str) -> String {
    let line = text
        .lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .trim_start_matches(['#', '-', '*', '>', ' '])
        .trim();
    let s: String = line.chars().take(60).collect();
    if s.is_empty() {
        "✓".to_string()
    } else {
        s
    }
}

fn lang_running(lang: &str) -> String {
    if lang == "en" { "running…".into() } else { "执行中…".into() }
}
fn lang_skip(lang: &str) -> String {
    if lang == "en" { "missed (skipped)".into() } else { "已错过，跳过".into() }
}
fn lang_failed(lang: &str) -> String {
    if lang == "en" { "failed (will retry next time)".into() } else { "失败，下次重试".into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_takes_first_nonempty_line_stripped() {
        assert_eq!(first_line_summary("\n\n# 今天 8 条\n详情..."), "今天 8 条");
        assert_eq!(first_line_summary("- 1 封待回邮件\n..."), "1 封待回邮件");
        assert_eq!(first_line_summary("   "), "✓");
    }

    #[test]
    fn action_prompt_marks_background_and_no_screenshot() {
        let p = build_action_prompt("收集 AI 新闻", "zh");
        assert!(p.contains("后台"));
        assert!(p.contains("没有屏幕截图"));
        assert!(p.contains("收集 AI 新闻"));
    }
}
