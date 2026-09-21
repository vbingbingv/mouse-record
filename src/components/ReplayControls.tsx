import { useState } from "react";

import type {
  EngineState,
  Recording,
  ReplayOptions,
  ReplayProgress,
} from "../types/automation";

interface ReplayControlsProps {
  engineState: EngineState;
  recording: Recording | null;
  progress: ReplayProgress | null;
  onStart: (options: ReplayOptions) => void;
  onStop: () => void;
}

export function ReplayControls({
  engineState,
  recording,
  progress,
  onStart,
  onStop,
}: ReplayControlsProps) {
  const [loops, setLoops] = useState(1);
  const [infinite, setInfinite] = useState(false);
  const [loopIntervalMs, setLoopIntervalMs] = useState(500);
  const [speed, setSpeed] = useState(1.0);

  const isIdle = engineState === "idle";
  const isBusy = engineState !== "idle";

  const hasActions = recording !== null && recording.actions.length > 0;
  const canStart = isIdle && hasActions;
  const canStop = engineState === "replaying" || engineState === "stopping";

  const start = () => {
    onStart({
      loops: infinite ? 1 : Math.max(1, Math.floor(loops)),
      loop_interval_ms: Math.max(0, Math.floor(loopIntervalMs)),
      speed,
      infinite,
    });
  };

  return (
    <section className="panel">
      <h2>回放</h2>

      <div className="form-row">
        <label>
          循环次数
          <input
            type="number"
            min={1}
            max={100000}
            value={infinite ? "" : loops}
            placeholder="∞"
            disabled={isBusy || infinite}
            onChange={(e) => setLoops(Number(e.target.value))}
          />
        </label>

        <label>
          循环间隔 (ms)
          <input
            type="number"
            min={0}
            step={100}
            value={loopIntervalMs}
            disabled={isBusy}
            onChange={(e) => setLoopIntervalMs(Number(e.target.value))}
          />
        </label>

        <label>
          速度
          <select
            value={speed}
            disabled={isBusy}
            onChange={(e) => setSpeed(Number(e.target.value))}
          >
            <option value={0.5}>0.5x</option>
            <option value={1.0}>1.0x</option>
            <option value={2.0}>2.0x</option>
            <option value={5.0}>5.0x</option>
          </select>
        </label>

        <label className="checkbox-label">
          <input
            type="checkbox"
            checked={infinite}
            disabled={isBusy}
            onChange={(e) => setInfinite(e.target.checked)}
          />
          无限循环
        </label>
      </div>

      <div className="button-row">
        <button className="btn primary" disabled={!canStart} onClick={start}>
          开始回放
        </button>
        <button className="btn danger" disabled={!canStop} onClick={onStop}>
          停止回放
        </button>
      </div>

      {progress && (
        <div className="progress-row">
          第 {progress.loop_index + 1}/{progress.infinite ? "∞" : progress.loops}{" "}
          轮 · 动作 {progress.action_index + 1}/{progress.action_count}
        </div>
      )}

      {engineState === "replaying" || engineState === "stopping" ? (
        <div className="hint">回放期间可按 ESC 紧急停止</div>
      ) : null}
    </section>
  );
}
