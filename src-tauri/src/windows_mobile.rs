//! 移动端窗口管理的 no-op 实现:移动 UI 是全屏主 webview,无多窗口/托盘概念。
//! 到点提醒的系统通知形态(前台服务 + 通知)是 Phase 2 设计课题。

#![allow(clippy::needless_pass_by_value)]

use tauri::AppHandle;

pub fn init(_app: &AppHandle) -> tauri::Result<()> {
    Ok(())
}

pub fn show_main(_app: &AppHandle) {}

pub fn hide_main(_app: &AppHandle) {}

pub fn show_card(_app: &AppHandle) {}

pub fn hide_card(_app: &AppHandle) {}

pub fn show_break(_app: &AppHandle, _fullscreen: bool) {}

pub fn hide_break(_app: &AppHandle) {}
