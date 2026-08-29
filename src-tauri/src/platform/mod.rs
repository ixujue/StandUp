//! 平台能力入口。Phase 1 仅实现 Windows;Android(Phase 2)与
//! macOS/Linux(Phase 3)在此接入各自的空闲采样与后台能力。

#[cfg(windows)]
pub mod win;

#[cfg(windows)]
pub use win::{spawn_event_thread, spawn_tick_thread};

#[cfg(not(windows))]
pub fn spawn_tick_thread(_tx: std::sync::mpsc::Sender<standup_core::Input>) {}

#[cfg(not(windows))]
pub fn spawn_event_thread(_tx: std::sync::mpsc::Sender<standup_core::Input>) {}
