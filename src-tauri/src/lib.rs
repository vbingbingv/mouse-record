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

    tauri::Builder::default()
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

                        if let Err(e) = source.listen_forever(sink.clone()) {
                            let _ = app_handle.emit("input-listener-error", e.to_string());
                        }

                        let _ = app_handle.emit("input-listener-status", false);
                        std::thread::sleep(LISTENER_RETRY_INTERVAL);
                    }
                })
                .expect("failed to spawn input listener thread");

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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
