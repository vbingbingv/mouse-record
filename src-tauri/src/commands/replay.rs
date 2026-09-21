use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, State};

use crate::error::AutomationError;
use crate::input::controller::MouseController;
use crate::input::enigo::EnigoMouseController;
use crate::model::recording::{Recording, ReplayOptions};
use crate::model::state::EngineState;
use crate::replay::engine::{ReplayEngine, ReplayOutcome};
use crate::AppState;

const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(200);

fn state_conflict(actual: EngineState) -> AutomationError {
    match actual {
        EngineState::Recording => AutomationError::AlreadyRecording,
        EngineState::Replaying | EngineState::Stopping => AutomationError::AlreadyReplaying,
        EngineState::Idle => AutomationError::NotReplaying,
    }
}

#[tauri::command]
pub fn start_replay(
    app: AppHandle,
    state: State<AppState>,
    recording: Recording,
    options: ReplayOptions,
) -> Result<(), AutomationError> {
    recording.validate()?;
    if recording.actions.is_empty() {
        return Err(AutomationError::InvalidRecording(
            "recording has no actions".into(),
        ));
    }
    options.validate()?;

    state
        .hub
        .transition_state(EngineState::Idle, EngineState::Replaying)
        .map_err(state_conflict)?;

    state.hub.reset_replay_stop();
    let stop = state.hub.replay_stop_flag();
    let hub = state.hub.clone();
    let app_handle = app.clone();

    std::thread::Builder::new()
        .name("replay".into())
        .spawn(move || {
            let replay_result = (|| {
                let mut controller: Box<dyn MouseController> = match EnigoMouseController::new() {
                    Ok(c) => Box::new(c),
                    Err(e) => {
                        let _ = app_handle.emit("replay-error", e.to_string());
                        return;
                    }
                };

                let mut last_emit = Instant::now();
                let app_for_progress = app_handle.clone();

                let result = ReplayEngine::run(
                    controller.as_mut(),
                    &recording,
                    &options,
                    &stop,
                    |progress| {
                        if last_emit.elapsed() >= PROGRESS_EMIT_INTERVAL {
                            last_emit = Instant::now();
                            let _ = app_for_progress.emit("replay-progress", progress);
                        }
                    },
                );

                match result {
                    Ok(ReplayOutcome::Completed) => {
                        let _ = app_handle.emit("replay-finished", ());
                    }
                    Ok(ReplayOutcome::Stopped) => {
                        let _ = app_handle.emit("replay-stopped", ());
                    }
                    Err(e) => {
                        let _ = app_handle.emit("replay-error", e.to_string());
                    }
                }
            })();

            let _ = replay_result;

            // 无论正常结束、停止还是出错，统一回到 Idle
            hub.set_state(EngineState::Idle);
            let _ = app_handle.emit("engine-state-changed", EngineState::Idle);
        })
        .map_err(|e| {
            state.hub.set_state(EngineState::Idle);
            AutomationError::InputBackend(format!("spawn replay thread: {e}"))
        })?;

    let _ = app.emit("engine-state-changed", EngineState::Replaying);
    let _ = app.emit("replay-started", ());
    Ok(())
}

#[tauri::command]
pub fn stop_replay(state: State<AppState>) -> Result<(), AutomationError> {
    match state.hub.state() {
        EngineState::Replaying => {
            let _ = state
                .hub
                .transition_state(EngineState::Replaying, EngineState::Stopping);
        }
        EngineState::Stopping => {}
        _ => {}
    }
    state.hub.replay_stop_flag().store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub fn get_engine_state(state: State<AppState>) -> EngineState {
    state.hub.state()
}

#[tauri::command]
pub fn get_listener_status(state: State<AppState>) -> bool {
    state.hub.is_listener_running()
}
