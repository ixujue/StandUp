//! StandUp Tauri 外壳:只负责接入平台能力,业务判定全部在 standup-core。

mod commands;
mod driver;
mod platform;
mod store;
mod tray;
mod windows;

use std::sync::mpsc;
use std::sync::Mutex;

use standup_core::{Core, Input};
use tauri::Manager;
use tauri_plugin_autostart::{ManagerExt as _, MacosLauncher};

use crate::store::Store;

/// 全局共享状态。core 只在 driver 循环线程内推进;
/// 托盘、前端 command、平台采样线程都通过 `tx` 汇入。
pub struct AppState {
    pub core: Mutex<Core>,
    pub store: Mutex<Store>,
    pub tx: mpsc::Sender<Input>,
}

pub fn run() {
    let (tx, rx) = mpsc::channel::<Input>();

    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(move |app| {
            let dir = app
                .path()
                .config_dir()
                .expect("无法定位配置目录")
                .join("standup");
            std::fs::create_dir_all(&dir)?;

            let store = Store::load(dir)?;
            let autostart_on = store.config.autostart;
            let core = Core::new(store.config.clone());
            app.manage(AppState {
                core: Mutex::new(core),
                store: Mutex::new(store),
                tx: tx.clone(),
            });

            let handle = app.handle().clone();

            // 平台输入源:1 秒空闲采样 + 锁屏/电源事件(D8)
            platform::spawn_tick_thread(tx.clone());
            platform::spawn_event_thread(tx.clone());

            // 状态机驱动循环:唯一推进 core 的线程
            driver::spawn(handle.clone(), rx);

            tray::setup(&handle)?;
            windows::init(&handle)?;

            // 开机自启按配置落地(D10 默认开)
            let autolaunch = handle.autolaunch();
            if autostart_on {
                autolaunch.enable()?;
            } else {
                autolaunch.disable()?;
            }

            windows::show_main(&handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::get_dashboard,
            commands::start_break,
            commands::dismiss_card,
            commands::end_break,
            commands::pause,
            commands::resume,
            commands::hide_main,
        ])
        .run(tauri::generate_context!())
        .expect("StandUp 运行失败");
}
