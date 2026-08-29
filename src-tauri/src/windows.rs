//! 三窗口管理(D9):设置主窗、休息卡片(右下角不抢焦点)、休息页。
//! 全部自绘无边框;窗口显隐只由 driver 依据 core 输出驱动。

use tauri::{AppHandle, LogicalPosition, Manager, WebviewUrl, WebviewWindowBuilder};

/// 启动时创建隐藏的 card / break 窗口(main 由 tauri.conf.json 声明)。
pub fn init(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window("card").is_none() {
        WebviewWindowBuilder::new(app, "card", WebviewUrl::App("card.html".into()))
            .title("StandUp")
            .inner_size(360.0, 132.0)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(false)
            .resizable(false)
            .transparent(true)
            .visible(false)
            .build()?;
    }
    if app.get_webview_window("break").is_none() {
        WebviewWindowBuilder::new(app, "break", WebviewUrl::App("break.html".into()))
            .title("StandUp")
            .inner_size(380.0, 460.0)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .visible(false)
            .build()?;
    }
    Ok(())
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn hide_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
}

/// 休息卡片:右下角弹出,不抢焦点(创建时 focused(false))。
pub fn show_card(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("card") {
        position_bottom_right(&w, 360.0, 132.0, 24.0);
        let _ = w.show();
    }
}

pub fn hide_card(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("card") {
        let _ = w.hide();
    }
}

/// 休息页:默认右下角小窗;全屏开关打开后覆盖主显示器(D7)。
pub fn show_break(app: &AppHandle, fullscreen: bool) {
    if let Some(w) = app.get_webview_window("break") {
        if fullscreen {
            let _ = w.set_fullscreen(true);
        } else {
            let _ = w.set_fullscreen(false);
            position_bottom_right(&w, 380.0, 460.0, 24.0);
        }
        let _ = w.show();
    }
}

pub fn hide_break(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("break") {
        let _ = w.set_fullscreen(false);
        let _ = w.hide();
    }
}

/// 定位到主屏右下角,大致避开任务栏(留出约三倍边距的高度)。
fn position_bottom_right(w: &tauri::WebviewWindow, width: f64, height: f64, margin: f64) {
    if let Some(monitor) = w.current_monitor().ok().flatten() {
        let scale = monitor.scale_factor();
        let logical = LogicalPosition::new(
            (monitor.size().width as f64 / scale) - width - margin,
            (monitor.size().height as f64 / scale) - height - margin * 3.0,
        );
        let _ = w.set_position(logical);
    }
}
