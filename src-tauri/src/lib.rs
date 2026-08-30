//! StandUp Tauri 外壳:只负责接入平台能力,业务判定全部在 standup-core。
//! 桌面端(Windows/macOS/Linux):托盘 + 多窗口 + 自启;
//! 移动端(Android):全屏主 webview 壳,感知与通知形态见 Phase 2。

mod commands;
mod driver;
mod platform;
mod secret;
mod store;
mod sync;

#[cfg(desktop)]
mod tray;
#[cfg(desktop)]
mod windows;
#[cfg(mobile)]
#[path = "windows_mobile.rs"]
mod windows;

use std::sync::mpsc;
use std::sync::Mutex;

use standup_core::{Core, Input};
use tauri::Manager;

#[cfg(desktop)]
use tauri_plugin_autostart::{MacosLauncher, ManagerExt as _};

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

    let builder = tauri::Builder::default();

    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_autostart::init(
        MacosLauncher::LaunchAgent,
        None,
    ));

    builder
        .setup(move |app| {
            let dir = app
                .path()
                .config_dir()
                .expect("无法定位配置目录")
                .join("standup");
            std::fs::create_dir_all(&dir)?;

            let store = Store::load(dir)?;
            let core = Core::new(store.config.clone());
            app.manage(AppState {
                core: Mutex::new(core),
                store: Mutex::new(store),
                tx: tx.clone(),
            });

            let handle = app.handle().clone();

            // 平台输入源:1 秒空闲采样 + 锁屏/电源事件(D8;移动端暂为空实现)
            platform::spawn_tick_thread(tx.clone());
            platform::spawn_event_thread(tx.clone());

            // 状态机驱动循环:唯一推进 core 的线程
            driver::spawn(handle.clone(), rx);

            // WebDAV 同步定时器(D13):启动后首推,此后每 15 分钟
            sync::spawn_timer(handle.clone());

            #[cfg(desktop)]
            {
                tray::setup(&handle)?;
                windows::init(&handle)?;

                // 开机自启按配置落地(D10 默认开)
                let autostart_on = handle
                    .state::<AppState>()
                    .store
                    .lock()
                    .unwrap()
                    .config
                    .autostart;
                let autolaunch = handle.autolaunch();
                if autostart_on {
                    autolaunch.enable()?;
                } else {
                    autolaunch.disable()?;
                }

                windows::show_main(&handle);
            }

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
            commands::get_sync_info,
            commands::save_sync_settings,
            commands::sync_now,
        ])
        .run(tauri::generate_context!())
        .expect("StandUp 运行失败");
}
