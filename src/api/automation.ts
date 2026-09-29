import { invoke } from "@tauri-apps/api/core";

import type {
  EngineState,
  PermissionKind,
  PermissionStatus,
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

/** 查询系统权限状态（只查询，不弹授权对话框）。 */
export function getPermissionStatus(): Promise<PermissionStatus> {
  return invoke("get_permission_status");
}

/** 申请权限：macOS 上未决定过时会弹系统授权对话框。 */
export function requestPermission(): Promise<PermissionStatus> {
  return invoke("request_permission");
}

/** 打开系统设置里对应的授权面板。 */
export function openPermissionSettings(kind: PermissionKind): Promise<void> {
  return invoke("open_permission_settings", { kind });
}

/** 重启应用（macOS 授权后需要重启进程才会生效）。 */
export function restartApp(): Promise<void> {
  return invoke("restart_app");
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
