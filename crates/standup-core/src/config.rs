//! 用户配置模型(D6 默认参数)。JSON 缺字段时逐项回退默认值。

use serde::{Deserialize, Serialize};

/// 全部用户配置,序列化到 `config.json`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// 久坐间隔(分钟)。
    pub reminder_interval_min: u32,
    /// 休息时长(分钟)。
    pub break_duration_min: u32,
    /// 空闲多久判定为"离开"(分钟)。
    pub idle_threshold_min: u32,
    /// 休息页全屏遮罩开关(默认右下角小窗)。
    pub fullscreen_break: bool,
    /// 休息页配色预设:aurora(夜航)/ forest(森野)/ dawn(晨雾)。
    pub break_theme: String,
    /// 开机自启(D10 默认开)。
    pub autostart: bool,
    /// 常驻模式(D12):忽略键鼠空闲、始终计时;锁屏/睡眠仍判离开。
    pub resident_mode: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            reminder_interval_min: 45,
            break_duration_min: 5,
            idle_threshold_min: 5,
            fullscreen_break: false,
            break_theme: "aurora".to_string(),
            autostart: true,
            resident_mode: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let cfg: Config = serde_json::from_str(r#"{"reminder_interval_min": 30}"#).unwrap();
        assert_eq!(cfg.reminder_interval_min, 30);
        assert_eq!(cfg.break_duration_min, 5);
        assert_eq!(cfg.idle_threshold_min, 5);
        assert!(!cfg.fullscreen_break);
        assert_eq!(cfg.break_theme, "aurora");
        assert!(cfg.autostart);
        assert!(!cfg.resident_mode);
    }
}
