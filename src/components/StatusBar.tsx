import type { EngineState, PermissionStatus } from "../types/automation";

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
  listenerError: string | null;
  permissions: PermissionStatus | null;
}

/// 全局输入监听不可用时的提示：macOS 是 TCC 授权问题，Windows 是提权问题。
/// 后端返回的原始错误（例如 CGEventTapCreate failed）放进 tooltip，
/// 便于在没有日志的情况下定位到底是哪一步失败。
function listenerWarning(
  permissions: PermissionStatus | null,
  listenerError: string | null,
): { text: string; title: string } {
  const title = listenerError
    ? `监听线程错误：${listenerError}`
    : "未获得所需权限";

  if (permissions?.platform === "macos") {
    return {
      text: "输入监听不可用：请在 系统设置 → 隐私与安全性 中授权本应用（见下方引导）",
      title,
    };
  }

  return {
    text: "输入监听不可用：请以管理员身份重新启动本程序",
    title,
  };
}

export function StatusBar({
  engineState,
  listenerOk,
  listenerError,
  permissions,
}: StatusBarProps) {
  const state = STATE_LABEL[engineState];
  const warning = listenerWarning(permissions, listenerError);

  return (
    <div className="status-bar">
      <span className={`state-badge ${state.className}`}>
        <span className="state-dot" />
        {state.text}
      </span>
      {!listenerOk && (
        <span className="listener-warning" title={warning.title}>
          {warning.text}
        </span>
      )}
    </div>
  );
}