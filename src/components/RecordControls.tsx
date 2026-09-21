import type { EngineState, RecordingStats } from "../types/automation";
import { formatDuration } from "./StatusBar";

interface RecordControlsProps {
  engineState: EngineState;
  stats: RecordingStats | null;
  onStart: () => void;
  onStop: () => void;
}

export function RecordControls({
  engineState,
  stats,
  onStart,
  onStop,
}: RecordControlsProps) {
  const isIdle = engineState === "idle";
  const isRecording = engineState === "recording";

  const duration =
    engineState === "recording" && stats ? stats.duration_ms : null;
  const actionCount =
    engineState === "recording" && stats ? stats.action_count : null;

  return (
    <section className="panel">
      <h2>录制</h2>

      <div className="stats-row">
        <div className="stat">
          <span className="stat-label">动作数量</span>
          <span className="stat-value">
            {actionCount !== null ? actionCount : "--"}
          </span>
        </div>
        <div className="stat">
          <span className="stat-label">录制时长</span>
          <span className="stat-value">
            {duration !== null ? formatDuration(duration) : "--:--.---"}
          </span>
        </div>
        <div className="stat">
          <span className="stat-label">鼠标位置</span>
          <span className="stat-value">
            {engineState === "recording" && stats?.mouse_x != null
              ? `(${Math.round(stats.mouse_x)}, ${Math.round(stats.mouse_y ?? 0)})`
              : "--"}
          </span>
        </div>
      </div>

      <div className="button-row">
        <button className="btn primary" disabled={!isIdle} onClick={onStart}>
          开始录制
        </button>
        <button className="btn danger" disabled={!isRecording} onClick={onStop}>
          停止录制
        </button>
      </div>
    </section>
  );
}
