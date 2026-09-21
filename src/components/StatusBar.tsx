import type { EngineState } from "../types/automation";

const STATE_LABEL: Record<EngineState, { text: string; className: string }> = {
  idle: { text: "空闲", className: "state-idle" },
  recording: { text: "录制中", className: "state-recording" },
  replaying: { text: "回放中", className: "state-replaying" },
  stopping: { text: "停止中", className: "state-stopping" },
};

export function formatDuration(ms: number): string {
  const totalSec = Math.floor(ms / 1000);
  const m = Math.floor(totalSec / 60);
  const s = totalSec % 60;
  const frac = ms % 1000;
  return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}.${String(frac).padStart(3, "0")}`;
}

interface StatusBarProps {
  engineState: EngineState;
  listenerOk: boolean;
}

export function StatusBar({ engineState, listenerOk }: StatusBarProps) {
  const state = STATE_LABEL[engineState];

  return (
    <div className="status-bar">
      <span className={`state-badge ${state.className}`}>
        <span className="state-dot" />
        {state.text}
      </span>
      {!listenerOk && (
        <span className="listener-warning" title="未获得系统辅助功能权限">
          输入监听不可用：请在 系统设置 → 隐私与安全性 → 辅助功能 中授权本应用后重启
        </span>
      )}
    </div>
  );
}
