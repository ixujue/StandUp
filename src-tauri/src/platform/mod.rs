//! 平台能力入口。Windows 为完整实现(D8);macOS/Linux 为 Phase 3 首批
//! 实验实现(空闲采样,锁屏/睡眠事件与 Wayland 支持待完善);
//! Android(Phase 2)接入 1 秒 Tick(前台计时,锁屏/后台由系统挂起,见 Phase 2 后台服务)。

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

/// 移动端(Android/iOS):无键鼠空闲概念,idle 恒为 0。
/// 1 秒 Tick 驱动计时与休息倒计时;APP 退后台/锁屏时系统挂起线程,
/// 恢复前台靠 last_tick_ms 增量累计,墙钟推进不丢时长。
#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub fn spawn_tick_thread(tx: std::sync::mpsc::Sender<standup_core::Input>) {
    use standup_core::Input;

    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
        tx.send(Input::Tick {
            now_ms: crate::driver::now_ms(),
            idle_secs: 0,
        })
        .ok();
    });
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub fn spawn_event_thread(_tx: std::sync::mpsc::Sender<standup_core::Input>) {}
