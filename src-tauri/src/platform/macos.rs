//! macOS 平台能力:CoreGraphics 事件源 1 秒空闲采样。
//! 锁屏/睡眠事件暂未接入:macOS 睡眠时进程冻结,唤醒后空闲时长自然超过阈值,
//! 由核心的"离开"判定兜底(D8 的 Windows 细则不适用;增强留待 Phase 3)。

use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use standup_core::Input;

use crate::driver::now_ms;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    /// kCGEventSourceStateCombinedSessionState = 0;kCGAnyInputEventType = ~0u64。
    fn CGEventSourceSecondsSinceLastEventType(state: u32, event_type: u64) -> f64;
}

pub fn spawn_tick_thread(tx: Sender<Input>) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(1));
        let idle = unsafe { CGEventSourceSecondsSinceLastEventType(0, u64::MAX) };
        tx.send(Input::Tick {
            now_ms: now_ms(),
            idle_secs: idle.max(0.0) as u64,
        })
        .ok();
    });
}

/// 与其他平台签名一致;macOS 暂无事件源,恢复累计由空闲判定兜底。
pub fn spawn_event_thread(_tx: Sender<Input>) {}
