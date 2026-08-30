//! 前端 command 桥:只转发 Input / 读取快照,不直接改状态。

use standup_core::{CardAction, Config, Input, Snapshot};
use tauri::{AppHandle, Manager, State};

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
    let break_theme = config.break_theme.clone();
    let now = driver::now_ms();
    {
        let mut st = state.store.lock().unwrap();
        st.save_config(&config).map_err(|e| e.to_string())?;
        // 本地修改时间前移(WebDAV LWW 比较基准,D13)
        st.sync_meta.settings.config_updated_at = now;
        let _ = st.sync_meta.save();
    }
    let _ = state.tx.send(Input::ConfigChanged(Box::new(config)));
    // 广播给常驻窗口:全局主题(本体与休息页)实时换肤
    tauri::Emitter::emit(
        &app,
        "config-changed",
        serde_json::json!({ "break_theme": break_theme }),
    )
    .ok();

    // 开机自启仅桌面端有概念(D10;移动端由系统后台策略决定)
    #[cfg(desktop)]
    {
        let autostart = {
            let st = state.store.lock().unwrap();
            st.config.autostart
        };
        use tauri_plugin_autostart::ManagerExt as _;
        let autolaunch = app.autolaunch();
        let _ = if autostart { autolaunch.enable() } else { autolaunch.disable() };
    }
    let _ = app; // 移动端无自启操作
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

// ---- WebDAV 同步(D13)----

/// 采用远端配置:与 save_config 相同的生效路径,但不改 LWW 时间戳(由调用方设定)。
pub fn apply_remote_config(app: &AppHandle, config: Config) -> Result<(), String> {
    let state: State<AppState> = app.state();
    let break_theme = config.break_theme.clone();
    state
        .store
        .lock()
        .unwrap()
        .save_config(&config)
        .map_err(|e| e.to_string())?;
    let _ = state.tx.send(Input::ConfigChanged(Box::new(config)));
    tauri::Emitter::emit(
        app,
        "config-changed",
        serde_json::json!({ "break_theme": break_theme }),
    )
    .ok();

    #[cfg(desktop)]
    {
        let autostart = state.store.lock().unwrap().config.autostart;
        use tauri_plugin_autostart::ManagerExt as _;
        let autolaunch = app.autolaunch();
        let _ = if autostart { autolaunch.enable() } else { autolaunch.disable() };
    }
    Ok(())
}

#[derive(serde::Serialize)]
pub struct SyncInfo {
    pub enabled: bool,
    pub url: String,
    pub username: String,
    pub has_password: bool,
    pub last_sync_at: u64,
    pub last_sync_ok: bool,
    pub last_error: String,
}

#[tauri::command]
pub fn get_sync_info(state: State<AppState>) -> SyncInfo {
    let st = state.store.lock().unwrap();
    let s = &st.sync_meta.settings;
    SyncInfo {
        enabled: s.enabled,
        url: s.url.clone(),
        username: s.username.clone(),
        has_password: crate::sync::get_password(&s.username)
            .map(|p| !p.is_empty())
            .unwrap_or(false),
        last_sync_at: s.last_sync_at,
        last_sync_ok: s.last_sync_ok,
        last_error: s.last_error.clone(),
    }
}

#[tauri::command]
pub fn save_sync_settings(
    app: AppHandle,
    state: State<AppState>,
    enabled: bool,
    url: String,
    username: String,
    password: Option<String>,
) -> Result<(), String> {
    {
        let mut st = state.store.lock().unwrap();
        let s = &mut st.sync_meta.settings;
        s.enabled = enabled;
        s.url = url.trim().trim_end_matches('/').to_string();
        // 账号变更时旧凭据不再适用,清掉
        if s.username != username {
            if !s.username.is_empty() {
                let _ = crate::sync::set_password(&s.username, "");
            }
            s.config_updated_at = driver::now_ms(); // 新端点无历史,以本机为准
        }
        s.username = username;
        st.sync_meta.save().map_err(|e| e.to_string())?;
    }
    if let Some(p) = password {
        let name = state.store.lock().unwrap().sync_meta.settings.username.clone();
        crate::sync::set_password(&name, &p)?;
    }
    let _ = app;
    Ok(())
}

#[tauri::command]
pub fn sync_now(app: AppHandle) -> Result<String, String> {
    let result = crate::sync::sync_once(&app);
    match &result {
        Ok(_) => {
            let _ = tauri::Emitter::emit(&app, "sync-changed", ());
        }
        Err(e) => crate::sync::record_failure(&app, e),
    }
    result
}
