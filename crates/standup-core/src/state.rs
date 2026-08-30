//! 久坐计时状态机(设计文档第二节)。
//!
//! 所有输入自带时间戳,由外壳驱动推进;本模块无线程、无 IO。
//! 关键语义(均已拍板):
//! - 提醒即一轮终点,轮次计时清零,每 45 分钟节奏不变(D7-F1);
//! - 空闲超阈值 / 锁屏 / 睡眠 → Away,计时清零重计(D6/D8);
//! - 休息页关闭(走完或提前)即回 Active 从零累计(D7);
//! - Paused 优先于一切自动转移,恢复时间到自动回 Active(D7-N1)。

use serde::Serialize;

use crate::config::Config;
use crate::events::FlowEvent;

/// 单次 Tick 允许计入累计的最大墙钟增量:超过视为挂钟异常
/// (进程挂起等),按 2 秒记;睡眠/锁屏由专门事件处理。
const MAX_TICK_DELTA_MS: u64 = 2_000;
/// 休息卡片无响应自动淡出时长(D7)。
const CARD_TIMEOUT_MS: u64 = 3 * 60_000;

/// 状态机状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum State {
    /// 久坐累计中。
    Active,
    /// 休息页打开,倒计时进行中。
    OnBreak,
    /// 离开:空闲超阈值 / 锁屏 / 睡眠,计时已清零。
    Away,
    /// 手动暂停,直到指定时间点。
    Paused,
}

/// 休息卡片上的按钮动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardAction {
    StartBreak,
    Dismiss,
}

/// 外壳 → 核心的输入。
#[derive(Debug)]
pub enum Input {
    /// 每秒一次的采样。
    Tick { now_ms: u64, idle_secs: u64 },
    SessionLock { now_ms: u64 },
    /// 无操作:恢复累计由下一次 Tick 的 idle 判定驱动。
    SessionUnlock { now_ms: u64 },
    /// 睡眠开始:即时置为 Away(D8),避免唤醒瞬间误报超长久坐。
    Suspend { now_ms: u64 },
    /// 无操作:同 SessionUnlock。
    Resume { now_ms: u64 },
    CardAction { now_ms: u64, action: CardAction },
    /// 休息页关闭;`completed = true` 表示倒计时自然走完。
    BreakEnd { now_ms: u64, completed: bool },
    /// 托盘"暂停 1 小时 / 到明天":绝对恢复时间点由外壳计算。
    PauseUntil { now_ms: u64, until_ms: u64 },
    ResumeNow { now_ms: u64 },
    ConfigChanged(Box<Config>),
}

/// 核心 → 外壳的输出,按返回顺序执行。
#[derive(Debug)]
pub enum Output {
    ShowCard,
    HideCard,
    ShowBreak { fullscreen: bool },
    CloseBreak,
    /// 状态或进度有变化,外壳据此刷新托盘 tooltip/图标与今日概览。
    StateChanged,
    Flow(FlowEvent),
}

/// 对外快照:托盘 tooltip / 图标两态、今日概览当前轮数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Snapshot {
    pub state: State,
    /// 当前轮已累计久坐毫秒数。
    pub seated_ms: u64,
    /// 仅 Active:距下一次提醒的毫秒数。
    pub next_reminder_in_ms: Option<u64>,
    /// 仅 Paused:恢复时间点。
    pub pause_until_ms: Option<u64>,
    pub card_visible: bool,
    /// 仅 OnBreak:休息剩余毫秒数。
    pub break_remaining_ms: Option<u64>,
}

/// 平台无关的状态机实例。
#[derive(Debug)]
pub struct Core {
    config: Config,
    state: State,
    /// 当前轮已累计久坐毫秒数。
    seated_ms: u64,
    /// 上一次 Tick 的墙钟,用于计算本轮增量。
    last_tick_ms: Option<u64>,
    /// 卡片自动淡出截止时间;Some = 卡片显示中。
    card_deadline_ms: Option<u64>,
    break_started_ms: Option<u64>,
    break_deadline_ms: Option<u64>,
    pause_until_ms: Option<u64>,
}

impl Core {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            state: State::Active,
            seated_ms: 0,
            last_tick_ms: None,
            card_deadline_ms: None,
            break_started_ms: None,
            break_deadline_ms: None,
            pause_until_ms: None,
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn state(&self) -> State {
        self.state
    }

    fn interval_ms(&self) -> u64 {
        u64::from(self.config.reminder_interval_min) * 60_000
    }

    fn threshold_ms(&self) -> u64 {
        u64::from(self.config.idle_threshold_min) * 60_000
    }

    fn break_ms(&self) -> u64 {
        u64::from(self.config.break_duration_min) * 60_000
    }

    /// 推进状态机,返回按序执行的输出。
    pub fn handle(&mut self, input: Input) -> Vec<Output> {
        match input {
            Input::Tick { now_ms, idle_secs } => self.on_tick(now_ms, idle_secs),
            Input::SessionLock { now_ms } => self.force_away(now_ms, 0),
            Input::Suspend { now_ms } => self.force_away(now_ms, 0),
            Input::SessionUnlock { .. } | Input::Resume { .. } => vec![],
            Input::CardAction { now_ms, action } => self.on_card_action(now_ms, action),
            Input::BreakEnd { now_ms, completed } => {
                if self.state == State::OnBreak {
                    self.finish_break(now_ms, completed)
                } else {
                    vec![]
                }
            }
            Input::PauseUntil { now_ms, until_ms } => self.on_pause(now_ms, until_ms),
            Input::ResumeNow { now_ms } => {
                if self.state == State::Paused {
                    self.resume_active(now_ms);
                    vec![Output::StateChanged]
                } else {
                    vec![]
                }
            }
            Input::ConfigChanged(cfg) => {
                self.config = *cfg;
                vec![Output::StateChanged]
            }
        }
    }

    /// 托盘 tooltip / 图标与今日概览的当前轮数据。
    pub fn snapshot(&self, now_ms: u64) -> Snapshot {
        Snapshot {
            state: self.state,
            seated_ms: self.seated_ms,
            next_reminder_in_ms: (self.state == State::Active)
                .then(|| self.interval_ms().saturating_sub(self.seated_ms)),
            pause_until_ms: (self.state == State::Paused)
                .then(|| self.pause_until_ms.unwrap_or(0)),
            card_visible: self.card_deadline_ms.is_some(),
            break_remaining_ms: (self.state == State::OnBreak)
                .then(|| self.break_deadline_ms.unwrap_or(0).saturating_sub(now_ms)),
        }
    }

    fn on_tick(&mut self, now: u64, idle_secs: u64) -> Vec<Output> {
        match self.state {
            State::Paused => {
                if now >= self.pause_until_ms.unwrap_or(u64::MAX) {
                    self.resume_active(now);
                    vec![Output::StateChanged]
                } else {
                    vec![]
                }
            }
            // 休息中忽略空闲事件(休息即离开),只看倒计时是否到点。
            State::OnBreak => {
                if now >= self.break_deadline_ms.unwrap_or(u64::MAX) {
                    self.finish_break(now, true)
                } else {
                    vec![]
                }
            }
            State::Away => {
                // 常驻模式(D12):不看空闲,直接回 Active 继续累计。
                if self.config.resident_mode || u64::from(idle_secs) * 1000 < self.threshold_ms() {
                    self.state = State::Active;
                    self.seated_ms = 0;
                    self.last_tick_ms = Some(now);
                    vec![Output::StateChanged]
                } else {
                    vec![]
                }
            }
            State::Active => self.on_tick_active(now, idle_secs),
        }
    }

    fn on_tick_active(&mut self, now: u64, idle_secs: u64) -> Vec<Output> {
        let delta = self
            .last_tick_ms
            .map_or(0, |t| now.saturating_sub(t).min(MAX_TICK_DELTA_MS));
        self.last_tick_ms = Some(now);
        self.seated_ms += delta;

        // 常驻模式(D12):空闲不判离开;锁屏/睡眠走 SessionLock/Suspend,仍会强制 Away。
        if !self.config.resident_mode && u64::from(idle_secs) * 1000 >= self.threshold_ms() {
            let idle_min = (u64::from(idle_secs) / 60) as u32;
            self.enter_away();
            let mut out = Vec::new();
            if self.card_deadline_ms.take().is_some() {
                out.push(Output::HideCard);
            }
            out.push(Output::Flow(FlowEvent::Away { at_ms: now, idle_min }));
            out.push(Output::StateChanged);
            return out;
        }

        let mut out = Vec::new();
        if self.card_deadline_ms.is_some_and(|d| now >= d) {
            self.card_deadline_ms = None;
            out.push(Output::HideCard);
            out.push(Output::StateChanged);
        }
        if self.seated_ms >= self.interval_ms() {
            let seated_min = (self.seated_ms / 60_000) as u32;
            self.seated_ms = 0; // 提醒即一轮终点(D7-F1)
            self.card_deadline_ms = Some(now + CARD_TIMEOUT_MS);
            out.push(Output::Flow(FlowEvent::Reminder { at_ms: now, seated_min }));
            out.push(Output::ShowCard);
            out.push(Output::StateChanged);
        }
        out
    }

    fn on_card_action(&mut self, now: u64, action: CardAction) -> Vec<Output> {
        match action {
            CardAction::StartBreak if self.state == State::Active => {
                self.card_deadline_ms = None;
                self.state = State::OnBreak;
                self.break_started_ms = Some(now);
                self.break_deadline_ms = Some(now + self.break_ms());
                vec![
                    Output::HideCard,
                    Output::ShowBreak { fullscreen: self.config.fullscreen_break },
                    Output::StateChanged,
                ]
            }
            CardAction::Dismiss => {
                if self.card_deadline_ms.take().is_some() {
                    vec![Output::HideCard, Output::StateChanged]
                } else {
                    vec![]
                }
            }
            _ => vec![],
        }
    }

    fn on_pause(&mut self, now: u64, until_ms: u64) -> Vec<Output> {
        let mut out = Vec::new();
        if self.state == State::OnBreak {
            out.extend(self.finish_break(now, false));
        }
        if self.card_deadline_ms.take().is_some() {
            out.push(Output::HideCard);
        }
        self.state = State::Paused;
        self.pause_until_ms = Some(until_ms);
        self.seated_ms = 0;
        self.last_tick_ms = Some(now);
        out.push(Output::StateChanged);
        out
    }

    /// 锁屏 / 睡眠:强制转 Away 并关闭卡片 / 休息页。
    /// Paused 与 Away 时保持原状(暂停意图优先,离开无需重复)。
    fn force_away(&mut self, now: u64, idle_min: u32) -> Vec<Output> {
        if self.state == State::Paused || self.state == State::Away {
            return vec![];
        }
        let mut out = Vec::new();
        if self.state == State::OnBreak {
            out.extend(self.finish_break(now, false));
        }
        if self.card_deadline_ms.take().is_some() {
            out.push(Output::HideCard);
        }
        self.enter_away();
        out.push(Output::Flow(FlowEvent::Away { at_ms: now, idle_min }));
        out.push(Output::StateChanged);
        out
    }

    fn finish_break(&mut self, now: u64, completed: bool) -> Vec<Output> {
        let started = self.break_started_ms.take().unwrap_or(now);
        self.break_deadline_ms = None;
        let planned_min = self.config.break_duration_min;
        let actual_min = (now.saturating_sub(started) / 60_000) as u32;
        self.resume_active(now);
        vec![
            Output::Flow(FlowEvent::Break {
                at_ms: now,
                planned_min,
                actual_min,
                completed,
            }),
            Output::CloseBreak,
            Output::StateChanged,
        ]
    }

    fn enter_away(&mut self) {
        self.state = State::Away;
        self.seated_ms = 0;
    }

    fn resume_active(&mut self, now: u64) {
        self.state = State::Active;
        self.pause_until_ms = None;
        self.seated_ms = 0;
        self.last_tick_ms = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::FlowEvent;

    const SEC: u64 = 1_000;
    const MIN: u64 = 60_000;

    fn core() -> Core {
        Core::new(Config::default())
    }

    /// 每秒推进一个 Tick(idle 固定),返回全部输出。
    fn run(core: &mut Core, t: &mut u64, mins: u32, idle_secs: u64) -> Vec<Output> {
        let mut out = Vec::new();
        let end = *t + u64::from(mins) * MIN;
        while *t < end {
            *t += SEC;
            out.extend(core.handle(Input::Tick { now_ms: *t, idle_secs }));
        }
        out
    }

    fn showed_card(outputs: &[Output]) -> bool {
        outputs.iter().any(|o| matches!(o, Output::ShowCard))
    }

    fn flows(outputs: &[Output]) -> Vec<FlowEvent> {
        outputs
            .iter()
            .filter_map(|o| match o {
                Output::Flow(e) => Some(e.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn reminder_at_default_interval() {
        let mut c = core();
        let mut t = 0;
        c.handle(Input::Tick { now_ms: t, idle_secs: 0 }); // 建立 last_tick 基准
        assert!(!showed_card(&run(&mut c, &mut t, 44, 0)));
        let out = run(&mut c, &mut t, 1, 0);
        assert!(showed_card(&out));
        assert_eq!(
            flows(&out).as_slice(),
            [FlowEvent::Reminder { at_ms: t, seated_min: 45 }]
        );
        assert_eq!(c.state(), State::Active); // 提醒只弹卡片,不改状态
    }

    #[test]
    fn reminder_repeats_each_interval_without_upgrade() {
        let mut c = core();
        let mut t = 0;
        c.handle(Input::Tick { now_ms: t, idle_secs: 0 });
        assert!(showed_card(&run(&mut c, &mut t, 45, 0)));
        assert!(!showed_card(&run(&mut c, &mut t, 44, 0))); // 下一轮未满,不重复
        assert!(showed_card(&run(&mut c, &mut t, 1, 0))); // 满 45 分钟再提醒
    }

    #[test]
    fn idle_over_threshold_goes_away_and_resets() {
        let mut c = core();
        let mut t = 0;
        c.handle(Input::Tick { now_ms: t, idle_secs: 0 });
        run(&mut c, &mut t, 10, 0);
        let out = c.handle(Input::Tick { now_ms: t + SEC, idle_secs: 5 * 60 });
        t += SEC;
        assert_eq!(c.state(), State::Away);
        assert_eq!(
            flows(&out).as_slice(),
            [FlowEvent::Away { at_ms: t, idle_min: 5 }]
        );
        // 新输入后回 Active,从零累计:44 分钟不应提醒
        c.handle(Input::Tick { now_ms: t + SEC, idle_secs: 0 });
        t += SEC;
        assert_eq!(c.state(), State::Active);
        assert!(!showed_card(&run(&mut c, &mut t, 44, 0)));
        assert!(showed_card(&run(&mut c, &mut t, 1, 0)));
    }

    #[test]
    fn short_idle_keeps_counting() {
        let mut c = core();
        let mut t = 0;
        c.handle(Input::Tick { now_ms: t, idle_secs: 0 });
        run(&mut c, &mut t, 10, 0);
        c.handle(Input::Tick { now_ms: t + SEC, idle_secs: 120 }); // 接水 2 分钟
        t += SEC;
        assert_eq!(c.state(), State::Active);
        assert!(showed_card(&run(&mut c, &mut t, 35, 0))); // 累计继续,满 45 即提醒
    }

    #[test]
    fn card_auto_hides_after_timeout() {
        let mut c = core();
        let mut t = 0;
        c.handle(Input::Tick { now_ms: t, idle_secs: 0 });
        assert!(showed_card(&run(&mut c, &mut t, 45, 0)));
        let out = run(&mut c, &mut t, 3, 0);
        assert!(out.iter().any(|o| matches!(o, Output::HideCard)));
        assert!(!c.snapshot(t).card_visible);
    }

    #[test]
    fn break_natural_completion() {
        let mut c = core();
        let mut t = 0;
        c.handle(Input::Tick { now_ms: t, idle_secs: 0 });
        assert!(showed_card(&run(&mut c, &mut t, 45, 0)));
        let out = c.handle(Input::CardAction { now_ms: t, action: CardAction::StartBreak });
        assert!(out.iter().any(|o| matches!(o, Output::ShowBreak { fullscreen: false })));
        assert_eq!(c.state(), State::OnBreak);
        let out = run(&mut c, &mut t, 5, 0);
        assert!(out.iter().any(|o| matches!(o, Output::CloseBreak)));
        assert_eq!(c.state(), State::Active);
        assert_eq!(
            flows(&out).as_slice(),
            [FlowEvent::Break { at_ms: t, planned_min: 5, actual_min: 5, completed: true }]
        );
    }

    #[test]
    fn break_early_end_is_not_completed() {
        let mut c = core();
        let mut t = 0;
        c.handle(Input::Tick { now_ms: t, idle_secs: 0 });
        run(&mut c, &mut t, 45, 0);
        c.handle(Input::CardAction { now_ms: t, action: CardAction::StartBreak });
        run(&mut c, &mut t, 2, 0);
        let out = c.handle(Input::BreakEnd { now_ms: t, completed: false });
        assert!(out.iter().any(|o| matches!(o, Output::CloseBreak)));
        assert_eq!(c.state(), State::Active);
        assert_eq!(
            flows(&out).as_slice(),
            [FlowEvent::Break { at_ms: t, planned_min: 5, actual_min: 2, completed: false }]
        );
    }

    #[test]
    fn lock_during_break_forces_away_and_closes_break() {
        let mut c = core();
        let mut t = 0;
        c.handle(Input::Tick { now_ms: t, idle_secs: 0 });
        run(&mut c, &mut t, 45, 0);
        c.handle(Input::CardAction { now_ms: t, action: CardAction::StartBreak });
        run(&mut c, &mut t, 1, 0);
        let out = c.handle(Input::SessionLock { now_ms: t });
        assert!(out.iter().any(|o| matches!(o, Output::CloseBreak)));
        assert!(out.iter().any(|o| matches!(o, Output::Flow(FlowEvent::Break { completed: false, .. }))));
        assert!(out.iter().any(|o| matches!(o, Output::Flow(FlowEvent::Away { .. }))));
        assert_eq!(c.state(), State::Away);
    }

    #[test]
    fn paused_suppresses_reminders_and_auto_resumes() {
        let mut c = core();
        let mut t = 0;
        c.handle(Input::Tick { now_ms: t, idle_secs: 0 });
        c.handle(Input::PauseUntil { now_ms: t, until_ms: t + 60 * MIN });
        assert_eq!(c.state(), State::Paused);
        assert!(!showed_card(&run(&mut c, &mut t, 60, 0))); // 暂停期间零提醒
        assert_eq!(c.state(), State::Active); // 到点自动恢复
        assert!(showed_card(&run(&mut c, &mut t, 45, 0)));
    }

    #[test]
    fn resume_now_works() {
        let mut c = core();
        let t = 0;
        c.handle(Input::PauseUntil { now_ms: t, until_ms: t + 60 * MIN });
        c.handle(Input::ResumeNow { now_ms: t });
        assert_eq!(c.state(), State::Active);
        assert_eq!(c.snapshot(t).pause_until_ms, None);
    }

    #[test]
    fn config_change_takes_effect() {
        let mut c = core();
        let mut t = 0;
        c.handle(Input::Tick { now_ms: t, idle_secs: 0 });
        let mut cfg = Config::default();
        cfg.reminder_interval_min = 15;
        cfg.fullscreen_break = true;
        c.handle(Input::ConfigChanged(Box::new(cfg)));
        assert!(showed_card(&run(&mut c, &mut t, 15, 0)));
        let out = c.handle(Input::CardAction { now_ms: t, action: CardAction::StartBreak });
        assert!(out.iter().any(|o| matches!(o, Output::ShowBreak { fullscreen: true })));
    }

    #[test]
    fn snapshot_reports_next_reminder() {
        let mut c = core();
        let t = 0;
        c.handle(Input::Tick { now_ms: t, idle_secs: 0 });
        c.handle(Input::Tick { now_ms: t + SEC, idle_secs: 0 });
        let s = c.snapshot(t + SEC);
        assert_eq!(s.state, State::Active);
        assert_eq!(s.next_reminder_in_ms, Some(45 * MIN - SEC));
    }

    #[test]
    fn resident_mode_ignores_idle_and_still_reminds() {
        let mut c = core();
        let mut t = 0;
        c.handle(Input::Tick { now_ms: t, idle_secs: 0 });
        let mut cfg = Config::default();
        cfg.resident_mode = true;
        c.handle(Input::ConfigChanged(Box::new(cfg)));
        // 全程无键鼠输入(idle 持续增长)照样累计并提醒
        let mut t2 = t;
        assert!(!showed_card(&run(&mut c, &mut t2, 44, 300)));
        let out = run(&mut c, &mut t2, 1, 300);
        assert!(showed_card(&out));
    }

    #[test]
    fn resident_mode_still_away_on_lock() {
        let mut c = core();
        let t = 0;
        let mut cfg = Config::default();
        cfg.resident_mode = true;
        c.handle(Input::ConfigChanged(Box::new(cfg)));
        c.handle(Input::Tick { now_ms: t, idle_secs: 0 });
        let out = c.handle(Input::SessionLock { now_ms: t + SEC });
        assert_eq!(c.state(), State::Away);
        assert!(out.iter().any(|o| matches!(o, Output::Flow(FlowEvent::Away { .. }))));
    }

    #[test]
    fn resident_mode_resumes_from_away_immediately() {
        let mut c = core();
        let mut t = 0;
        c.handle(Input::Tick { now_ms: t, idle_secs: 0 });
        let out = c.handle(Input::Tick { now_ms: t + SEC, idle_secs: 10 * 60 });
        t += SEC;
        assert_eq!(c.state(), State::Away);
        // 开启常驻模式:下一个 Tick 不看空闲,直接回 Active
        let mut cfg = Config::default();
        cfg.resident_mode = true;
        c.handle(Input::ConfigChanged(Box::new(cfg)));
        let out = c.handle(Input::Tick { now_ms: t + SEC, idle_secs: 10 * 60 });
        t += SEC;
        assert_eq!(c.state(), State::Active);
        assert!(out.iter().any(|o| matches!(o, Output::StateChanged)));
    }

    #[test]
    fn resident_mode_off_restores_idle_detection() {
        let mut c = core();
        let mut t = 0;
        c.handle(Input::Tick { now_ms: t, idle_secs: 0 });
        let mut cfg = Config::default();
        cfg.resident_mode = true;
        c.handle(Input::ConfigChanged(Box::new(cfg.clone())));
        run(&mut c, &mut t, 5, 0);
        cfg.resident_mode = false;
        c.handle(Input::ConfigChanged(Box::new(cfg)));
        let out = c.handle(Input::Tick { now_ms: t + SEC, idle_secs: 6 * 60 });
        assert_eq!(c.state(), State::Away);
        assert!(out.iter().any(|o| matches!(o, Output::Flow(FlowEvent::Away { .. }))));
    }
}
