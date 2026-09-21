mod commands;
mod error;
mod input;
mod model;
mod platform;
mod recorder;
mod replay;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::Emitter;

use model::recording::Recording;
use recorder::hub::RecorderHub;
#[derive(Clone)]
pub struct AppState {
    pub hub: Arc<RecorderHub>,
    pub current_recording: Arc<Mutex<Option<Recording>>>,
}

const LISTENER_RETRY_INTERVAL: Duration = Duration::from_secs(5);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let hub = RecorderHub::new();
    let state = AppState {
        hub: hub.clone(),
        current_recording: Arc::new(Mutex::new(None)),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let sink = hub.event_sink();
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
            commands::replay::get_engine_state,
            commands::replay::get_listener_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
