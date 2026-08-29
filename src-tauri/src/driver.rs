//! 状态机驱动循环:唯一消费 Input、推进 Core、执行 Output 的线程。

use std::sync::mpsc;

use standup_core::Output;
use tauri::{AppHandle, Emitter, Manager};

use crate::{tray, windows, AppState};

pub fn spawn(app: AppHandle, rx: mpsc::Receiver<standup_core::Input>) {
    std::thread::spawn(move || {
        while let Ok(input) = rx.recv() {
            let outputs = {
                let st = app.state::<AppState>();
                let mut core = st.core.lock().unwrap();
                core.handle(input)
            };
            for out in outputs {
                apply(&app, out);
            }
        }
    });
}

fn apply(app: &AppHandle, out: Output) {
    match out {
        Output::ShowCard => {
            windows::show_card(app);
            let interval = app.state::<AppState>().core.lock().unwrap().config().reminder_interval_min;
            let _ = app.emit("card-shown", serde_json::json!({ "interval_min": interval }));
        }
        Output::HideCard => windows::hide_card(app),
        Output::ShowBreak { fullscreen } => {
            windows::show_break(app, fullscreen);
            // 把权威剩余时长与当前配色交给前端做流畅渲染;判定仍以 core 为准。
            let st = app.state::<AppState>();
            let snap = st.core.lock().unwrap().snapshot(now_ms());
            let theme = st.store.lock().unwrap().config.break_theme.clone();
            log::info!("break-started: remaining={:?} theme={theme}", snap.break_remaining_ms);
            let _ = app.emit(
                "break-started",
                serde_json::json!({ "remaining_ms": snap.break_remaining_ms, "theme": theme }),
            );
        }
        Output::CloseBreak => windows::hide_break(app),
        Output::StateChanged => {
            tray::update(app);
            let snap = app.state::<AppState>().core.lock().unwrap().snapshot(now_ms());
            let _ = app.emit("state-changed", &snap);
        }
        Output::Flow(event) => {
            if let Err(err) = app.state::<AppState>().store.lock().unwrap().append(event) {
                log::error!("写事件流水失败: {err}");
            }
        }
    }
}

/// 统一的毫秒时间戳(Unix epoch)。
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// 暂停恢复时间点(D7-N1):1 小时后(false)或明天 00:00(true)。
pub fn pause_until_ms(to_tomorrow: bool) -> u64 {
    use chrono::{Duration as ChronoDuration, Local, TimeZone};
    let now = Local::now();
    let until = if to_tomorrow {
        let tomorrow = (now + ChronoDuration::days(1))
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        Local.from_local_datetime(&tomorrow)
            .single()
            .unwrap_or(now + ChronoDuration::days(1))
    } else {
        now + ChronoDuration::hours(1)
    };
    until.timestamp_millis() as u64
}
