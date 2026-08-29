//! Windows 平台能力(D8):`GetLastInputInfo` 1 秒轮询 + 锁屏/电源事件
//! (message-only window)。两个线程只向 driver 通道发 Input,不做业务判断。

use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::RemoteDesktop::{
    WTSRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION,
};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, GetWindowLongPtrW,
    RegisterClassW, SetWindowLongPtrW, TranslateMessage, CW_USEDEFAULT, GWLP_USERDATA, MSG,
    WNDCLASSW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_POWERBROADCAST, WM_WTSSESSION_CHANGE,
    PBT_APMRESUMEAUTOMATIC, PBT_APMSUSPEND,
};

use standup_core::Input;

use crate::driver::now_ms;

/// `WM_WTSSESSION_CHANGE` 的 wParam 取值(windows crate 未导出,取自 WTS API 文档)。
const WTS_SESSION_LOCK: usize = 0x7;
const WTS_SESSION_UNLOCK: usize = 0x8;

pub fn spawn_tick_thread(tx: Sender<Input>) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(1));
        tx.send(Input::Tick { now_ms: now_ms(), idle_secs: idle_secs() }).ok();
    });
}

/// 系统级键鼠空闲秒数(GetLastInputInfo,与 D8 一致)。
fn idle_secs() -> u64 {
    unsafe {
        let mut info = LASTINPUTINFO::default();
        info.cbSize = std::mem::size_of::<LASTINPUTINFO>() as u32;
        if GetLastInputInfo(&mut info).as_bool() {
            GetTickCount().wrapping_sub(info.dwTime) as u64 / 1000
        } else {
            0
        }
    }
}

/// message-only window:接收会话锁与电源广播,不显示、不进任务栏。
pub fn spawn_event_thread(tx: Sender<Input>) {
    thread::spawn(move || unsafe { message_loop(tx) });
}

unsafe fn message_loop(tx: Sender<Input>) {
    let Ok(hmodule) = GetModuleHandleW(None) else { return };
    let hinstance = HINSTANCE(hmodule.0);
    let class_name = w!("standup_events");

    let wc = WNDCLASSW {
        lpfnWndProc: Some(wndproc),
        hInstance: hinstance,
        lpszClassName: class_name,
        ..Default::default()
    };
    RegisterClassW(&wc);

    let Ok(hwnd) = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        class_name,
        w!("standup"),
        WINDOW_STYLE::default(),
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        None,
        None,
        hinstance,
        None,
    ) else {
        return;
    };

    SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(Box::new(tx)) as isize);
    let _ = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);

    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}

unsafe fn tx_of(hwnd: HWND) -> Option<&'static Sender<Input>> {
    (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Sender<Input>).as_ref()
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_WTSSESSION_CHANGE => {
            if let Some(tx) = tx_of(hwnd) {
                let _ = match wparam.0 {
                    x if x == WTS_SESSION_LOCK => tx.send(Input::SessionLock { now_ms: now_ms() }),
                    x if x == WTS_SESSION_UNLOCK => {
                        tx.send(Input::SessionUnlock { now_ms: now_ms() })
                    }
                    _ => Ok(()),
                };
            }
            LRESULT(0)
        }
        WM_POWERBROADCAST => {
            if let Some(tx) = tx_of(hwnd) {
                let _ = match wparam.0 {
                    x if x == PBT_APMSUSPEND as usize => {
                        tx.send(Input::Suspend { now_ms: now_ms() })
                    }
                    x if x == PBT_APMRESUMEAUTOMATIC as usize => {
                        tx.send(Input::Resume { now_ms: now_ms() })
                    }
                    _ => Ok(()),
                };
            }
            LRESULT(1)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
