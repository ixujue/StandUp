//! 配置与事件流水持久化(D10):`%APPDATA%\standup\` 下
//! `config.json`(缺字段回退默认)与 `events.jsonl`(追加写)。

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use standup_core::{Config, DaySummary, FlowEvent};

pub struct Store {
    pub config: Config,
    config_path: PathBuf,
    events_path: PathBuf,
    /// 内存流水副本,供今日概览聚合;文件为准,启动时回读。
    events: Vec<FlowEvent>,
}

impl Store {
    pub fn load(dir: PathBuf) -> std::io::Result<Self> {
        let config_path = dir.join("config.json");
        let events_path = dir.join("events.jsonl");

        let config = fs::read_to_string(&config_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let events = fs::read_to_string(&events_path)
            .map(|s| {
                s.lines()
                    .filter_map(|line| serde_json::from_str(line).ok())
                    .collect()
            })
            .unwrap_or_default();

        Ok(Self {
            config,
            config_path,
            events_path,
            events,
        })
    }

    pub fn save_config(&mut self, config: &Config) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(config)?;
        fs::write(&self.config_path, json)?;
        self.config = config.clone();
        Ok(())
    }

    pub fn append(&mut self, event: FlowEvent) -> std::io::Result<()> {
        let line = serde_json::to_string(&event)?;
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.events_path)?;
        writeln!(f, "{line}")?;
        self.events.push(event);
        Ok(())
    }

    pub fn today_summary(&self) -> DaySummary {
        let (start, end) = today_range_ms();
        standup_core::summarize_day(&self.events, start, end)
    }
}

/// 本地日历日(今日 00:00 → 次日 00:00)的毫秒区间。
fn today_range_ms() -> (u64, u64) {
    use chrono::{Duration, Local, TimeZone};
    let now = Local::now();
    let today = now.date_naive();
    let start = Local
        .from_local_datetime(&today.and_hms_opt(0, 0, 0).unwrap())
        .single()
        .unwrap_or(now);
    let end = start + Duration::days(1);
    (start.timestamp_millis() as u64, end.timestamp_millis() as u64)
}
