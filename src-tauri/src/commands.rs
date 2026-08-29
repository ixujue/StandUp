//! 前端 command 桥:只转发 Input / 读取快照,不直接改状态。

use standup_core::{CardAction, Config, Input, Snapshot};
use tauri::{AppHandle, State};

use crate::{driver, windows, AppState};

#[derive(serde::Serialize)]
pub struct Dashboard {
    pub summary: standup_core::DaySummary,
    pub snapshot: Snapshot,
}

#[tauri::command]
pub fn get_config(state: State<AppState>) -> Config {
    state.store.lock().unwrap().config.clone()
}

#[tauri::command]
pub fn save_config(app: AppHandle, state: State<AppState>, config: Config) -> Result<(), String> {
    let autostart = config.autostart;
    let break_theme = config.break_theme.clone();
    state
        .store
        .lock()
        .unwrap()
        .save_config(&config)
        .map_err(|e| e.to_string())?;
    let _ = state.tx.send(Input::ConfigChanged(Box::new(config)));
    // 广播给常驻窗口:全局主题(本体与休息页)实时换肤
    tauri::Emitter::emit(
        &app,
        "config-changed",
        serde_json::json!({ "break_theme": break_theme }),
    )
    .ok();

    use tauri_plugin_autostart::ManagerExt as _;
    let autolaunch = app.autolaunch();
    let _ = if autostart { autolaunch.enable() } else { autolaunch.disable() };
    Ok(())
}

#[tauri::command]
pub fn get_dashboard(state: State<AppState>) -> Dashboard {
    let summary = state.store.lock().unwrap().today_summary();
    let snapshot = state.core.lock().unwrap().snapshot(driver::now_ms());
    Dashboard { summary, snapshot }
}

#[tauri::command]
pub fn start_break(state: State<AppState>) {
    let _ = state.tx.send(Input::CardAction {
        now_ms: driver::now_ms(),
        action: CardAction::StartBreak,
    });
}

#[tauri::command]
pub fn dismiss_card(state: State<AppState>) {
    let _ = state.tx.send(Input::CardAction {
        now_ms: driver::now_ms(),
        action: CardAction::Dismiss,
    });
}

#[tauri::command]
pub fn end_break(state: State<AppState>, completed: bool) {
    let _ = state.tx.send(Input::BreakEnd { now_ms: driver::now_ms(), completed });
}

#[tauri::command]
pub fn pause(state: State<AppState>, kind: String) {
    let until_ms = driver::pause_until_ms(kind.as_str() == "today");
    let _ = state.tx.send(Input::PauseUntil { now_ms: driver::now_ms(), until_ms });
}

#[tauri::command]
pub fn resume(state: State<AppState>) {
    let _ = state.tx.send(Input::ResumeNow { now_ms: driver::now_ms() });
}

#[tauri::command]
pub fn hide_main(app: AppHandle) {
    windows::hide_main(&app);
}
