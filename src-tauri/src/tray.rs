//! 托盘(D10):左键单击开设置窗;右键菜单 = 状态项 → 立即休息 →
//! 暂停 1 小时 / 到明天 → 恢复 → 设置… → 退出。tooltip 动态显示久坐状态。

use chrono::{Local, TimeZone};
use standup_core::{CardAction, Input, State};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::{driver, windows, AppState};

/// 菜单句柄缓存,供 `update` 刷新状态文本与菜单项可用性。
pub struct TrayHandles {
    pub status: MenuItem<Wry>,
    pub rest: MenuItem<Wry>,
    pub resume: MenuItem<Wry>,
    pub pause1h: MenuItem<Wry>,
    pub pause_today: MenuItem<Wry>,
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let status = MenuItem::with_id(app, "status", "下次提醒:45 分钟后", false, None::<&str>)?;
    let rest = MenuItem::with_id(app, "rest", "立即休息", true, None::<&str>)?;
    let pause1h = MenuItem::with_id(app, "pause-1h", "暂停 1 小时", true, None::<&str>)?;
    let pause_today = MenuItem::with_id(app, "pause-today", "暂停到明天", true, None::<&str>)?;
    let resume = MenuItem::with_id(app, "resume", "恢复提醒", false, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "设置…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
            &status, &sep1, &rest, &pause1h, &pause_today, &resume, &sep2, &settings, &quit,
        ],
    )?;

    TrayIconBuilder::with_id("standup-tray")
        .icon(app.default_window_icon().expect("缺少应用图标").clone())
        .tooltip("StandUp")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "settings" => windows::show_main(app),
            "rest" => send(
                app,
                Input::CardAction { now_ms: driver::now_ms(), action: CardAction::StartBreak },
            ),
            "pause-1h" => send(
                app,
                Input::PauseUntil { now_ms: driver::now_ms(), until_ms: driver::pause_until_ms(false) },
            ),
            "pause-today" => send(
                app,
                Input::PauseUntil { now_ms: driver::now_ms(), until_ms: driver::pause_until_ms(true) },
            ),
            "resume" => send(app, Input::ResumeNow { now_ms: driver::now_ms() }),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                windows::show_main(tray.app_handle());
            }
        })
        .build(app)?;

    app.manage(TrayHandles {
        status,
        rest,
        resume,
        pause1h,
        pause_today,
    });
    Ok(())
}

/// 按 core 快照刷新 tooltip、状态文本与菜单项可用性。
/// (图标两态灰化待图标资源就绪后接入,README 有记录。)
pub fn update(app: &AppHandle) {
    let Some(tray) = app.tray_by_id("standup-tray") else { return };
    let st = app.state::<AppState>();
    let snap = st.core.lock().unwrap().snapshot(driver::now_ms());

    let (text, paused) = match snap.state {
        State::Active => {
            let mins = (snap.next_reminder_in_ms.unwrap_or(0) / 60_000).max(1);
            (format!("下次提醒:{mins} 分钟后"), false)
        }
        State::OnBreak => ("休息中,慢一点".to_string(), false),
        State::Away => ("人不在,随时等你回来".to_string(), false),
        State::Paused => {
            let until = Local
                .timestamp_millis_opt(snap.pause_until_ms.unwrap_or(0) as i64)
                .single()
                .map(|t| t.format("%H:%M").to_string())
                .unwrap_or_default();
            (format!("已暂停至 {until}"), true)
        }
    };
    let _ = tray.set_tooltip(Some(format!("StandUp · {text}")));

    let h = app.state::<TrayHandles>();
    let _ = h.status.set_text(&text);
    let _ = h.rest.set_enabled(!paused && snap.state == State::Active);
    let _ = h.resume.set_enabled(paused);
    let _ = h.pause1h.set_enabled(!paused);
    let _ = h.pause_today.set_enabled(!paused);
}

fn send(app: &AppHandle, input: Input) {
    let _ = app.state::<AppState>().tx.send(input);
}
