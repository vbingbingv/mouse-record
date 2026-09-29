export type MouseButton = "Left" | "Middle" | "Right" | "Back" | "Forward";

export type MouseAction =
  | { type: "Move"; x: number; y: number }
  | { type: "ButtonDown"; button: MouseButton }
  | { type: "ButtonUp"; button: MouseButton }
  | { type: "Wheel"; delta_x: number; delta_y: number };

export interface RecordedAction {
  /** 距上一个已记录动作的时间（微秒） */
  delay_us: number;
  action: MouseAction;
}

export interface Recording {
  version: number;
  actions: RecordedAction[];
}

export interface ReplayOptions {
  loops: number;
  loop_interval_ms: number;
  speed: number;
  /** 无限循环：为 true 时忽略 loops，回放到被停止为止 */
  infinite: boolean;
}

export type EngineState = "idle" | "recording" | "replaying" | "stopping";

/** 需要引导授权的权限项（macOS：输入监控 / 辅助功能）。 */
export type PermissionKind = "listenInput" | "postEvents";

/**
 * 平台权限状态。
 * Windows 恒为已授权（提权由启动时的 UAC 完成）；macOS 需要在系统设置里授权。
 */
export interface PermissionStatus {
  platform: "macos" | "windows";
  listen_input: boolean;
  post_events: boolean;
}

export interface RecordingStats {
  duration_ms: number;
  action_count: number;
  mouse_x: number | null;
  mouse_y: number | null;
  last_action: RecordedAction | null;
}

export interface ReplayProgress {
  loop_index: number;
  loops: number;
  /** 无限循环时为 true，此时 loops 无意义 */
  infinite: boolean;
  action_index: number;
  action_count: number;
}

export interface RecordingMeta {
  name: string;
  version: number;
  action_count: number;
  duration_ms: number;
}
