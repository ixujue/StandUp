//! 平台能力入口。Windows 为完整实现(D8);macOS/Linux 为 Phase 3 首批
//! 实验实现(空闲采样,锁屏/睡眠事件与 Wayland 支持待完善);
//! Android(Phase 2)在此接入各自的空闲采样与后台能力。

#[cfg(windows)]
pub mod win;

#[cfg(windows)]
pub use win::{spawn_event_thread, spawn_tick_thread};

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "macos")]
pub use macos::{spawn_event_thread, spawn_tick_thread};

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "linux")]
pub use linux::{spawn_event_thread, spawn_tick_thread};

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub fn spawn_tick_thread(_tx: std::sync::mpsc::Sender<standup_core::Input>) {}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub fn spawn_event_thread(_tx: std::sync::mpsc::Sender<standup_core::Input>) {}
