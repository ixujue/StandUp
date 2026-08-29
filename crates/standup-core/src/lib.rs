//! StandUp 核心库:久坐计时状态机、配置模型与事件流水模型。
//!
//! 平台无关:不依赖任何系统 API;所有输入自带时间戳,由平台外壳
//! 以固定节奏驱动 [`Core::handle`] 推进,可完全单元测试。

pub mod config;
pub mod events;
pub mod state;

pub use config::Config;
pub use events::{summarize_day, DaySummary, FlowEvent};
pub use state::{CardAction, Core, Input, Output, Snapshot, State};
