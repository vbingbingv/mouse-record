use std::fs;
use std::path::PathBuf;

use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::AutomationError;
use crate::model::recording::Recording;
use crate::model::state::EngineState;
use crate::AppState;

const NAME_MAX_LEN: usize = 100;

#[derive(Debug, Clone, serde::Serialize)]
pub struct RecordingMeta {
    pub name: String,
    pub version: u32,
    pub action_count: usize,
    pub duration_ms: u64,
}

fn state_conflict(actual: EngineState) -> AutomationError {
    match actual {
        EngineState::Recording => AutomationError::AlreadyRecording,
        EngineState::Replaying | EngineState::Stopping => AutomationError::AlreadyReplaying,
        EngineState::Idle => AutomationError::NotRecording,
    }
}

fn sanitize_name(name: &str) -> Result<String, AutomationError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AutomationError::InvalidFile(
            "recording name must not be empty".into(),
        ));
    }
    if trimmed.len() > NAME_MAX_LEN {
        return Err(AutomationError::InvalidFile(format!(
            "recording name too long (max {NAME_MAX_LEN} chars)"
        )));
    }
    let cleaned: String = trimmed
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | ' ' | '.'))
        .collect();
    if cleaned.trim().is_empty() || cleaned.starts_with('.') {
        return Err(AutomationError::InvalidFile(
            "recording name contains no usable characters".into(),
        ));
    }
    Ok(cleaned)
}

fn recordings_dir(app: &AppHandle) -> Result<PathBuf, AutomationError> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| AutomationError::Io(format!("resolve app data dir: {e}")))?;
    Ok(base.join("recordings"))
}

fn meta_of(name: &str, recording: &Recording) -> RecordingMeta {
    RecordingMeta {
        name: name.to_string(),
        version: recording.version,
        action_count: recording.actions.len(),
        duration_ms: recording.total_duration_ms(),
    }
}

#[tauri::command]
pub fn start_recording(app: AppHandle, state: State<AppState>) -> Result<(), AutomationError> {
    state
        .hub
        .transition_state(EngineState::Idle, EngineState::Recording)
        .map_err(state_conflict)?;

    if !state.hub.is_listener_running() {
        state.hub.set_state(EngineState::Idle);
        return Err(AutomationError::ListenerUnavailable(
            "input listener is not running (check accessibility permission)".into(),
        ));
    }

    let app_for_stats = app.clone();
    let result = state.hub.start_session(move |stats| {
        let _ = app_for_stats.emit("recording-stats", stats);
    });

    if let Err(e) = result {
        state.hub.set_state(EngineState::Idle);
        return Err(e);
    }

    let _ = app.emit("engine-state-changed", EngineState::Recording);
    let _ = app.emit("recording-started", ());
    Ok(())
}

#[tauri::command]
pub fn stop_recording(
    app: AppHandle,
    state: State<AppState>,
) -> Result<Recording, AutomationError> {
    state
        .hub
        .transition_state(EngineState::Recording, EngineState::Idle)
        .map_err(state_conflict)?;

    let recording = state.hub.stop_session()?;

    *state
        .current_recording
        .lock()
        .expect("current recording lock poisoned") = Some(recording.clone());

    let _ = app.emit("engine-state-changed", EngineState::Idle);
    let _ = app.emit("recording-stopped", &recording);
    Ok(recording)
}

#[tauri::command]
pub fn save_recording(
    app: AppHandle,
    state: State<AppState>,
    name: String,
) -> Result<RecordingMeta, AutomationError> {
    let recording = state
        .current_recording
        .lock()
        .expect("current recording lock poisoned")
        .clone()
        .ok_or_else(|| {
            AutomationError::InvalidRecording("no recording available to save".into())
        })?;

    let name = sanitize_name(&name)?;
    let dir = recordings_dir(&app)?;
    fs::create_dir_all(&dir)?;

    let path = dir.join(format!("{name}.json"));
    let json = serde_json::to_string_pretty(&recording)?;
    fs::write(&path, json)?;

    Ok(meta_of(&name, &recording))
}

#[tauri::command]
pub fn list_recordings(app: AppHandle) -> Result<Vec<RecordingMeta>, AutomationError> {
    let dir = recordings_dir(&app)?;
    let mut metas = Vec::new();

    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(recording) = serde_json::from_str::<Recording>(&content) {
                    metas.push(meta_of(stem, &recording));
                }
            }
        }
    }

    metas.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(metas)
}

#[tauri::command]
pub fn load_recording(
    app: AppHandle,
    state: State<AppState>,
    name: String,
) -> Result<Recording, AutomationError> {
    let name = sanitize_name(&name)?;
    let path = recordings_dir(&app)?.join(format!("{name}.json"));

    let content = fs::read_to_string(&path)
        .map_err(|_| AutomationError::InvalidFile(format!("recording '{name}' not found")))?;
    let mut recording: Recording = serde_json::from_str(&content)?;
    recording.validate()?;
    // v1（delay_ms）加载时升级为 v2（delay_us），保证后续保存的文件格式一致
    recording.migrate_legacy();

    *state
        .current_recording
        .lock()
        .expect("current recording lock poisoned") = Some(recording.clone());

    Ok(recording)
}

#[tauri::command]
pub fn delete_recording(app: AppHandle, name: String) -> Result<(), AutomationError> {
    let name = sanitize_name(&name)?;
    let path = recordings_dir(&app)?.join(format!("{name}.json"));
    fs::remove_file(&path)
        .map_err(|_| AutomationError::InvalidFile(format!("recording '{name}' not found")))?;
    Ok(())
}
