mod commands;
mod error;
mod hotkey;
mod input;
mod model;
mod platform;
mod recorder;
mod replay;
mod ui;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::Emitter;

use hotkey::HotkeyDispatcher;
use model::recording::{Recording, ReplayOptions};
use recorder::hub::RecorderHub;
#[derive(Clone)]
pub struct AppState {
    pub hub: Arc<RecorderHub>,
    pub current_recording: Arc<Mutex<Option<Recording>>>,
    /// 当前回放选项：UI 每次改动都会同步过来，热键回放直接沿用。
    pub last_replay_options: Arc<Mutex<ReplayOptions>>,
}

const LISTENER_RETRY_INTERVAL: Duration = Duration::from_secs(5);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let hub = RecorderHub::new();
    let state = AppState {
        hub: hub.clone(),
        current_recording: Arc::new(Mutex::new(None)),
        last_replay_options: Arc::new(Mutex::new(ReplayOptions::default())),
    };
    let hotkey_state = state.clone();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .setup(move |app| {
            let app_handle = app.handle().clone();

            // 热键动作走独立线程，避免在输入钩子回调里做阻塞操作
            let dispatcher = HotkeyDispatcher::new(app_handle.clone(), hotkey_state);
            let sink = hub.event_sink(move |hotkey| dispatcher.dispatch(hotkey));
            let listener_running = hub.listener_running_flag();

            // 常驻全局输入监听线程：失败（如 macOS 未授权辅助功能）时自动重试
            std::thread::Builder::new()
                .name("input-listener".into())
                .spawn(move || {
                    let source = input::platform_source(listener_running);
                    loop {
                        let _ = app_handle.emit("input-listener-status", true);
                        // 每轮重试都刷新一次权限状态：用户在系统设置里打开开关后，
                        // 界面无需手动刷新即可看到变化（成功拿到 tap 后不再进入下一轮）
                        let _ =
                            app_handle.emit("permission-status", platform::permissions::status());

                        if let Err(e) = source.listen_forever(sink.clone()) {
                            let _ = app_handle.emit("input-listener-error", e.to_string());
                        }

                        let _ = app_handle.emit("input-listener-status", false);
                        std::thread::sleep(LISTENER_RETRY_INTERVAL);
                    }
                })
                .expect("failed to spawn input listener thread");

            // macOS：启动时申请 TCC 授权（对应 Windows 清单里的 UAC 提权）。
            // 申请接口可能阻塞到用户回应，放独立线程，不拖慢启动。
            #[cfg(target_os = "macos")]
            {
                let app_for_permissions = app.handle().clone();
                std::thread::Builder::new()
                    .name("permission-request".into())
                    .spawn(move || {
                        let status = platform::permissions::request();
                        let _ = app_for_permissions.emit("permission-status", status);
                    })
                    .expect("failed to spawn permission request thread");
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::recording::start_recording,
            commands::recording::stop_recording,
            commands::recording::save_recording,
            commands::recording::list_recordings,
            commands::recording::load_recording,
            commands::recording::delete_recording,
            commands::replay::start_replay,
            commands::replay::stop_replay,
            commands::replay::set_replay_options,
            commands::replay::get_engine_state,
            commands::replay::get_listener_status,
            commands::permissions::get_permission_status,
            commands::permissions::request_permission,
            commands::permissions::open_permission_settings,
            commands::permissions::restart_app,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        // macOS：点 Dock 图标（NSApplicationDelegate 的 applicationShouldHandleReopen）时
        // 必须自己把窗口找回来 —— 录制/回放期间窗口是隐藏的，
        // 全局热键不可用时这是用户唯一的入口。其它平台不会触发该事件。
        if let tauri::RunEvent::Reopen { .. } = event {
            ui::show_main_window(app_handle);
        }
    });
}
