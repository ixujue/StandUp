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

// Android / iOS:暂无系统级空闲 API,以 1 秒 Tick(idle=0)驱动状态机推进,
// 否则 core 永远收不到 Tick,久坐不计时、提醒与休息倒计时都不会触发。
// 事件源(锁屏/前后台)待 Phase 2 接入。

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub fn spawn_tick_thread(tx: std::sync::mpsc::Sender<standup_core::Input>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
        if tx
            .send(standup_core::Input::Tick {
                now_ms: crate::driver::now_ms(),
                idle_secs: 0,
            })
            .is_err()
        {
            log::info!("移动端采样线程退出:driver 通道已关闭");
            break;
        }
    });
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub fn spawn_event_thread(_tx: std::sync::mpsc::Sender<standup_core::Input>) {}
