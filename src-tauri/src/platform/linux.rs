//! Linux 平台能力:X11 下用 XScreenSaver 扩展采样空闲;Wayland 暂无通用空闲
//! 协议,XOpenDisplay 失败时自动降级为"无空闲检测"(恒返回 0:提醒照常,
//! 不会自动暂停),Phase 3 再接入 Wayland idle-notify。
//! 锁屏/睡眠事件暂未接入,由空闲判定兜底(同 macOS 注)。

use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use standup_core::Input;
use x11::xlib;
use x11::xss;

use crate::driver::now_ms;

pub fn spawn_tick_thread(tx: Sender<Input>) {
    thread::spawn(move || {
        let idle_source = X11Idle::new();
        loop {
            thread::sleep(Duration::from_secs(1));
            let idle_secs = idle_source.as_ref().map_or(0, |s| s.idle_secs());
            tx.send(Input::Tick {
                now_ms: now_ms(),
                idle_secs,
            })
            .ok();
        }
    });
}

pub fn spawn_event_thread(_tx: Sender<Input>) {}

struct X11Idle {
    display: std::ptr::NonNull<xlib::Display>,
    info: *mut xss::XScreenSaverInfo,
    root: xlib::Drawable,
}

// Xlib 不跨线程;采样线程独占此连接。
unsafe impl Send for X11Idle {}

impl X11Idle {
    fn new() -> Option<Self> {
        unsafe {
            let display = xlib::XOpenDisplay(std::ptr::null());
            if display.is_null() {
                return None; // Wayland / 无 X 环境:降级
            }
            let mut ev_base = 0;
            let mut err_base = 0;
            if xss::XScreenSaverQueryExtension(display, &mut ev_base, &mut err_base) == 0 {
                xlib::XCloseDisplay(display);
                return None; // 扩展不可用:降级
            }
            let info = xss::XScreenSaverAllocInfo();
            if info.is_null() {
                xlib::XCloseDisplay(display);
                return None;
            }
            Some(Self {
                display: std::ptr::NonNull::new_unchecked(display),
                info,
                root: xlib::XDefaultRootWindow(display),
            })
        }
    }

    fn idle_secs(&self) -> u64 {
        unsafe {
            xss::XScreenSaverQueryInfo(self.display.as_ptr(), self.root, self.info);
            (*self.info).idle / 1000
        }
    }
}

impl Drop for X11Idle {
    fn drop(&mut self) {
        unsafe {
            if !self.info.is_null() {
                xlib::XFree(self.info as *mut std::os::raw::c_void);
            }
            xlib::XCloseDisplay(self.display.as_ptr());
        }
    }
}
