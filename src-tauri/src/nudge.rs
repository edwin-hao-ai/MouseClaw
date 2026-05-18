//! 主动提醒规则引擎 (v0.1.27 P3)
//!
//! 每 60s 跑一次：读 presence buffer + DnD/cooldown 状态 → 决定是否 emit
//! 一条 nudge 给前端。**完全本地，0 LLM 调用，0 token 成本**。
//!
//! ### 教养三铁律（DESIGN.md 同步）
//!   1. 永不打断深度工作 —— 心流检测命中时全部 nudge 排队
//!   2. 永不重叠 / 永不堆积 —— 同一时刻只发 1 条
//!   3. 永远可以"今天闭嘴" —— 单条 nudge 30min cooldown，前端可发 dismiss
//!
//! ### MVP 三条规则
//!   - Stretch (健康) :: 鼠标 ≥ 90min 没大幅移动 → "站起来动动"
//!   - Stuck (工作 ⭐) :: IDE 前台 + 5min 没敲键 + 鼠标乱晃 → "卡住了？召唤我吗？"
//!   - LateNight (情感) :: 本地时间 ≥ 23:30 + 仍活跃 → "该睡了"

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use chrono::Local;
use tauri::{AppHandle, Emitter};
use tokio::time::sleep;

use crate::events::{NudgeKind, NudgePayload, EV_NUDGE};
use crate::presence::PresenceBuffer;

const RULE_INTERVAL_SECS: u64 = 60;
/// 同一类 nudge 至少间隔多久才会再次触发
const COOLDOWN_SECS: i64 = 30 * 60;
/// 心流保护：最近 30min 里键盘活跃 sample 占比阈值
const FOCUS_PROTECTION_HITS: usize = 50; // 60 sample 里 50 个活跃 ≈ 83%
const FOCUS_PROTECTION_WINDOW: u64 = 30 * 60;

/// IDE bundle id 关键字（大小写不敏感子串匹配）。
/// 主要的代码编辑器都覆盖；不全也无所谓 —— 不命中只是不触发 Stuck，无害。
const IDE_BUNDLE_NEEDLES: &[&str] = &[
    "vscode", "code-",       // VS Code / Cursor / Windsurf
    "xcode",                 // Xcode
    "jetbrains", "intellij", "pycharm", "webstorm", "rustrover", "goland",
    "android.studio",
    "sublime", "neovim", "vim", "emacs", "zed",
];

#[derive(Debug, Default)]
pub struct NudgeState {
    /// 上次每种 nudge 发的时间戳，用来 cooldown
    last_fired: HashMap<NudgeKind, i64>,
    /// 用户休息到何时（unix 秒）—— PetMenu 💤 / nudge 的"今天闭嘴" 设
    nap_until_ts: i64,
    /// v0.1.32 · 上次发"早安"的本地 epoch-day（now_ts / 86400）。
    /// 用来保证 GoodMorning 每天只发一次。
    pub last_morning_day: i64,
    /// v0.1.32 · 上次发"午餐"的本地 epoch-day。Lunch 每天只发一次。
    pub last_lunch_day: i64,
}

impl NudgeState {
    pub fn new() -> Self { Self::default() }

    pub fn set_nap_until(&mut self, ts: i64) {
        self.nap_until_ts = ts;
    }

    pub fn in_nap(&self, now: i64) -> bool {
        self.nap_until_ts > now
    }

    pub fn cooldown_ok(&self, kind: NudgeKind, now: i64) -> bool {
        match self.last_fired.get(&kind) {
            None => true,
            Some(t) => now - t > COOLDOWN_SECS,
        }
    }

    pub fn mark_fired(&mut self, kind: NudgeKind, now: i64) {
        self.last_fired.insert(kind, now);
        // v0.1.32 · 早安 / 午餐 每天只发一次 —— 用 epoch-day 防重复
        match kind {
            NudgeKind::GoodMorning => self.last_morning_day = now / 86400,
            NudgeKind::Lunch       => self.last_lunch_day   = now / 86400,
            _ => {}
        }
    }
}

/// 给定 buffer + 当前 state，返回应该发的 nudge（取最高优先级一条）。
/// 优先级：Stretch (健康) > Stuck (工作) > LateNight (情感)
/// 这个函数是 pure logic —— 单测覆盖核心规则。
pub fn pick_nudge(
    buf: &PresenceBuffer, state: &NudgeState, now_ts: i64, lang_en: bool,
) -> Option<NudgePayload> {
    // 0. DnD / Nap 直接跳过所有 nudge
    if state.in_nap(now_ts) { return None; }

    // 1. 心流保护：最近 30min 高强度敲键 → 排队所有提醒
    if buf.keyboard_active_count(now_ts, FOCUS_PROTECTION_WINDOW) >= FOCUS_PROTECTION_HITS {
        return None;
    }

    let Some(latest) = buf.latest() else { return None; };
    // v0.1.32 · 共享给 Lunch / GoodMorning / Water 用的预计算
    let today_epoch_day = now_ts / 86400; // 本地时区简化处理
    let user_present = latest.secs_since_keyboard < 5.0 * 60.0
                       || latest.secs_since_mouse_move < 5.0 * 60.0;

    // 2. Stretch (久坐)
    //    鼠标 >= 90min 没动 → "站起来动一动"
    if state.cooldown_ok(NudgeKind::Stretch, now_ts)
        && latest.mouse_idle_for(90.0 * 60.0)
    {
        return Some(NudgePayload {
            kind: NudgeKind::Stretch,
            message: if lang_en {
                "🧘 Stretch break? You've been sitting for 90 minutes.".into()
            } else {
                "🧘 站起来动一动？已经坐了 90 分钟啦".into()
            },
            cta_label: None,
            cta_action: None,
        });
    }

    // 3. Stuck (卡壳侦测) ⭐ MouseClaw 杀手锏
    //    IDE 前台 ≥ 5min + 键盘几乎不响 + 鼠标却乱动 → 主动喂 LLM 入口
    let ide_hits = buf.focused_bundle_count_matching(now_ts, 5 * 60, IDE_BUNDLE_NEEDLES);
    if state.cooldown_ok(NudgeKind::Stuck, now_ts)
        && ide_hits >= 8                                     // 10 个 sample 里 ≥ 8 个 IDE 在前台
        && latest.secs_since_keyboard > 5.0 * 60.0           // 5min 没敲键
        && latest.secs_since_mouse_move < 30.0               // 鼠标却在动
    {
        return Some(NudgePayload {
            kind: NudgeKind::Stuck,
            message: if lang_en {
                "🤔 Stuck? Want to ask me?".into()
            } else {
                "🤔 卡住了？要不要召唤一下我？".into()
            },
            cta_label: Some(if lang_en { "Summon".into() } else { "召唤".into() }),
            cta_action: Some("summon".into()),
        });
    }

    // 4. LateNight (深夜劝睡)
    if state.cooldown_ok(NudgeKind::LateNight, now_ts)
        && (latest.hour >= 23 || latest.hour == 0)
    {
        return Some(NudgePayload {
            kind: NudgeKind::LateNight,
            message: if lang_en {
                "🌙 It's getting late… time to sleep?".into()
            } else {
                "🌙 已经很晚了…该睡了吧？".into()
            },
            cta_label: None,
            cta_action: None,
        });
    }

    // 5. v0.1.32 · Water (喝水) —— 健康类。
    //    触发条件：用户在电脑前（5min 内有任何键鼠输入）+ cooldown OK。
    //    早期版本只在连续敲键时触发，但喝水跟敲不敲键盘无关 —— 看视频 / 读文档 /
    //    设计 / 浏览的人也得喝水。改成只看"present 信号"。
    //    cooldown 用默认 30min，每半小时提醒一次 sip。
    if state.cooldown_ok(NudgeKind::Water, now_ts) && user_present {
        return Some(NudgePayload {
            kind: NudgeKind::Water,
            message: if lang_en {
                "💧 Time for a sip of water?".into()
            } else {
                "💧 喝口水吧？".into()
            },
            cta_label: None,
            cta_action: None,
        });
    }

    // 6. v0.1.32 · Lunch (午餐) —— 跟鼠标动不动无关，到点该吃就该吃。
    //    触发：本地时间 12:00-13:00 + 用户当下在线（5min 有键鼠输入）
    //    + 今天还没发过。每天只发一次（同 Morning 模式）。
    if latest.hour == 12
        && state.last_lunch_day != today_epoch_day
        && user_present
    {
        return Some(NudgePayload {
            kind: NudgeKind::Lunch,
            message: if lang_en {
                "🍕 Lunch break? It's past noon.".into()
            } else {
                "🍕 吃午饭啦？已经过 12 点了".into()
            },
            cta_label: None,
            cta_action: None,
        });
    }

    // 7. v0.1.32 · GoodMorning (早安) —— 每天第一次解锁屏幕（启动 app）
    //    判断：last_morning_date != today → 触发
    if state.last_morning_day != today_epoch_day
        && (latest.hour >= 6 && latest.hour <= 11)
    {
        return Some(NudgePayload {
            kind: NudgeKind::GoodMorning,
            message: if lang_en {
                "☀️ Morning! Ready to take on today?".into()
            } else {
                "☀️ 早安！今天准备搞什么大事？".into()
            },
            cta_label: None,
            cta_action: None,
        });
    }

    None
}

// ──────────────────────────────────────────────────────────────────
// Public spawn / state
// ──────────────────────────────────────────────────────────────────

pub fn spawn(
    app: AppHandle,
    buffer: Arc<RwLock<PresenceBuffer>>,
    state: Arc<RwLock<NudgeState>>,
) {
    tauri::async_runtime::spawn(async move {
        // 等几分钟让 buffer 攒点数据，再开始评估规则。
        // 否则一上来 latest=None / 5min 内的所有规则都假触发。
        sleep(Duration::from_secs(180)).await;

        loop {
            sleep(Duration::from_secs(RULE_INTERVAL_SECS)).await;
            let now = Local::now().timestamp();

            let lang_en = crate::config::Config::load().language == "en";

            let nudge = {
                let buf_g = match buffer.read() { Ok(g) => g, Err(_) => continue };
                let st_g  = match state.read()  { Ok(g) => g, Err(_) => continue };
                pick_nudge(&buf_g, &st_g, now, lang_en)
            };

            if let Some(payload) = nudge {
                // 标记 fired —— 防止同一规则连发
                if let Ok(mut st) = state.write() {
                    st.mark_fired(payload.kind, now);
                }
                // 老鼠先出来 —— 不然用户根本看不到 nudge bubble。
                // v0.1.32 · 用 _at_anchor 版本：避免 nudge 把 overlay 搬到鼠标位置
                // 然后启用 cursor_follow，那样气泡会跟着鼠标走 + 被屏幕边切半。
                crate::overlay::show_mouse_at_anchor(&app);
                if let Err(e) = app.emit(EV_NUDGE, &payload) {
                    eprintln!("[mouseclaw] nudge emit failed: {e}");
                } else {
                    println!("[mouseclaw] 🔔 nudge fired: {:?}", payload.kind);
                }
            }
        }
    });
    println!("[mouseclaw] 🦞 nudge engine spawned ({RULE_INTERVAL_SECS}s eval · {COOLDOWN_SECS}s cooldown)");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presence::PresenceSample;

    fn s(ts: i64, bundle: &str, kb: f64, mv: f64, hour: u8) -> PresenceSample {
        PresenceSample {
            ts_secs: ts, frontmost_bundle: bundle.into(),
            secs_since_keyboard: kb, secs_since_mouse_move: mv, hour,
        }
    }

    fn fresh_buf(samples: Vec<PresenceSample>) -> PresenceBuffer {
        let mut b = PresenceBuffer::new();
        for s in samples { b.push(s); }
        b
    }

    #[test]
    fn no_nudge_when_buffer_empty() {
        let buf = PresenceBuffer::new();
        let st = NudgeState::new();
        assert!(pick_nudge(&buf, &st, 1000, false).is_none());
    }

    #[test]
    fn no_nudge_when_in_nap() {
        let now = 1000;
        let buf = fresh_buf(vec![s(now, "x", 5000.0, 5500.0, 14)]); // 久坐
        let mut st = NudgeState::new();
        st.set_nap_until(now + 600); // nap 还有 10min
        assert!(pick_nudge(&buf, &st, now, false).is_none());
    }

    #[test]
    fn stretch_fires_on_long_mouse_idle() {
        let now = 1000;
        // 5500s = 91.6min mouse idle → 触发
        let buf = fresh_buf(vec![s(now, "com.apple.Safari", 0.0, 5500.0, 14)]);
        let st = NudgeState::new();
        let n = pick_nudge(&buf, &st, now, false).expect("应该触发 stretch");
        assert_eq!(n.kind, NudgeKind::Stretch);
    }

    #[test]
    fn stretch_cooldown_blocks_refire() {
        let now = 2000;
        let buf = fresh_buf(vec![s(now, "com.apple.Safari", 0.0, 5500.0, 14)]);
        let mut st = NudgeState::new();
        st.mark_fired(NudgeKind::Stretch, now - 100); // 100s 前刚发过
        assert!(pick_nudge(&buf, &st, now, false).is_none());
    }

    #[test]
    fn stuck_fires_when_ide_focused_silent_but_mouse_moving() {
        let now = 10_000;
        // 10 个最近 5min 内 sample，全 VS Code 前台
        let samples: Vec<_> = (0..10).map(|i| {
            // 最近一个 sample 鼠标在动 (mv=10s)，键盘 6min 没响
            let mv = if i == 9 { 10.0 } else { 0.0 };
            let kb = if i == 9 { 6.0 * 60.0 } else { 0.0 };
            s(now - (10 - i as i64) * 30, "com.microsoft.VSCode", kb, mv, 14)
        }).collect();
        let buf = fresh_buf(samples);
        let st = NudgeState::new();
        let n = pick_nudge(&buf, &st, now, false).expect("应该触发 stuck");
        assert_eq!(n.kind, NudgeKind::Stuck);
        assert_eq!(n.cta_action.as_deref(), Some("summon"));
    }

    #[test]
    fn stuck_does_not_fire_when_not_in_ide() {
        let now = 10_000;
        let samples: Vec<_> = (0..10).map(|i| {
            s(now - (10 - i as i64) * 30, "com.apple.Safari", 6.0 * 60.0, 10.0, 14)
        }).collect();
        let buf = fresh_buf(samples);
        let st = NudgeState::new();
        // 没在 IDE → 不该触发 stuck；mouse 没久静止 → 也不该触发 stretch
        // hour=14 → 也不该触发 late-night → None
        assert!(pick_nudge(&buf, &st, now, false).is_none());
    }

    #[test]
    fn late_night_fires_after_2330() {
        let now = 50_000;
        let buf = fresh_buf(vec![s(now, "com.apple.Safari", 5.0, 5.0, 23)]);
        let st = NudgeState::new();
        let n = pick_nudge(&buf, &st, now, false).expect("应该触发 late-night");
        assert_eq!(n.kind, NudgeKind::LateNight);
    }

    #[test]
    fn focus_protection_suppresses_during_intense_typing() {
        let now = 100_000;
        // 60 个 sample，全部键盘活跃 → 命中心流保护
        let samples: Vec<_> = (0..60).map(|i| {
            s(now - (60 - i as i64) * 30, "com.microsoft.VSCode", 1.0, 1.0, 14)
        }).collect();
        let buf = fresh_buf(samples);
        let st = NudgeState::new();
        assert!(pick_nudge(&buf, &st, now, false).is_none(),
                "心流保护命中时所有 nudge 都该被压制");
    }

    #[test]
    fn cta_action_present_only_for_stuck() {
        let now = 1000;
        // late-night
        let buf = fresh_buf(vec![s(now, "x", 0.0, 0.0, 23)]);
        let st = NudgeState::new();
        let n = pick_nudge(&buf, &st, now, false).unwrap();
        assert_eq!(n.kind, NudgeKind::LateNight);
        assert!(n.cta_action.is_none());
    }
}
