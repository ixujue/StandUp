//! 事件流水模型(D6-B2 / D8):reminder / break / away 三类,
//! 外壳追加写 `events.jsonl`;今日概览从流水聚合。

use serde::{Deserialize, Serialize};

/// 流水事件,JSONL 每行一条,`t` 为类型标签。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum FlowEvent {
    /// 到点提醒,`seated_min` 为本轮久坐分钟数(提醒即一轮终点)。
    Reminder { at_ms: u64, seated_min: u32 },
    /// 一次休息结束;`completed = false` 表示提前结束或被锁屏/睡眠中断。
    Break {
        at_ms: u64,
        planned_min: u32,
        actual_min: u32,
        completed: bool,
    },
    /// 判定离开(空闲超阈值 / 锁屏 / 睡眠);锁屏与睡眠型 `idle_min` 记 0 表示未测量。
    Away { at_ms: u64, idle_min: u32 },
}

/// 今日概览(D6-B2)。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct DaySummary {
    /// 今日各轮久坐合计(分钟)。
    pub seated_min: u32,
    pub reminders: u32,
    pub breaks: u32,
    /// 实际休息合计(分钟)。
    pub break_min: u32,
}

/// 聚合 `[day_start_ms, day_end_ms)` 区间内的流水为今日概览。
pub fn summarize_day(events: &[FlowEvent], day_start_ms: u64, day_end_ms: u64) -> DaySummary {
    let mut s = DaySummary::default();
    let in_day = |at: u64| at >= day_start_ms && at < day_end_ms;
    for e in events {
        match e {
            FlowEvent::Reminder { at_ms, seated_min } if in_day(*at_ms) => {
                s.seated_min += seated_min;
                s.reminders += 1;
            }
            FlowEvent::Break { at_ms, actual_min, .. } if in_day(*at_ms) => {
                s.breaks += 1;
                s.break_min += actual_min;
            }
            _ => {}
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_filters_by_day_and_sums() {
        let events = vec![
            FlowEvent::Reminder { at_ms: 100, seated_min: 45 },
            FlowEvent::Break { at_ms: 200, planned_min: 5, actual_min: 5, completed: true },
            FlowEvent::Reminder { at_ms: 9_999, seated_min: 10 }, // 区间外
            FlowEvent::Away { at_ms: 150, idle_min: 3 },
        ];
        let s = summarize_day(&events, 0, 1_000);
        assert_eq!(s.seated_min, 45);
        assert_eq!(s.reminders, 1);
        assert_eq!(s.breaks, 1);
        assert_eq!(s.break_min, 5);
    }

    #[test]
    fn flow_events_serialize_with_type_tag() {
        let e = FlowEvent::Reminder { at_ms: 42, seated_min: 45 };
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains(r#""t":"reminder""#), "unexpected: {json}");
    }
}
