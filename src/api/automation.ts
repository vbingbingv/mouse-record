import { invoke } from "@tauri-apps/api/core";

import type {
  EngineState,
  Recording,
  RecordingMeta,
  ReplayOptions,
} from "../types/automation";

export function startRecording(): Promise<void> {
  return invoke("start_recording");
}

export function stopRecording(): Promise<Recording> {
  return invoke("stop_recording");
}

export function startReplay(
  recording: Recording,
  options: ReplayOptions,
): Promise<void> {
  return invoke("start_replay", { recording, options });
}

export function stopReplay(): Promise<void> {
  return invoke("stop_replay");
}

/**
 * 把界面上的回放参数同步给后端：热键回放时窗口已隐藏，
 * 后端只能用这份值（否则会退化成默认的单次循环）。
 */
export function setReplayOptions(options: ReplayOptions): Promise<void> {
  return invoke("set_replay_options", { options });
}

export function getEngineState(): Promise<EngineState> {
  return invoke("get_engine_state");
}

export function getListenerStatus(): Promise<boolean> {
  return invoke("get_listener_status");
}

export function saveRecording(name: string): Promise<RecordingMeta> {
  return invoke("save_recording", { name });
}

export function listRecordings(): Promise<RecordingMeta[]> {
  return invoke("list_recordings");
}

export function loadRecording(name: string): Promise<Recording> {
  return invoke("load_recording", { name });
}

export function deleteRecording(name: string): Promise<void> {
  return invoke("delete_recording", { name });
}
