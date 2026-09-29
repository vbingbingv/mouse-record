import { useEffect, useState } from "react";

import type { PermissionKind, PermissionStatus } from "../types/automation";

interface PermissionBannerProps {
  permissions: PermissionStatus;
  listenerOk: boolean;
  onRequest: () => void;
  onRefresh: () => void;
  onOpenSettings: (kind: PermissionKind) => void;
  onRestart: () => void;
}

interface PermissionItem {
  kind: PermissionKind;
  label: string;
  detail: string;
  granted: boolean;
  /** 未授权时的后果，用来解释「为什么点了没反应」 */
  impact: string;
}

function items(permissions: PermissionStatus): PermissionItem[] {
  return [
    {
      kind: "listenInput",
      label: "输入监控",
      detail: "监听鼠标与键盘，录制和全局快捷键 Control+8 / Control+9 都依赖它",
      granted: permissions.listen_input,
      impact: "未授权：无法录制，且 Ctrl+8 / Ctrl+9 完全没反应",
    },
    {
      kind: "postEvents",
      label: "辅助功能",
      detail: "合成鼠标事件",
      granted: permissions.post_events,
      impact: "未授权：回放时鼠标不会动",
    },
  ];
}

function formatTime(date: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

/**
 * macOS 的授权引导（对应 Windows 启动时的 UAC 弹窗）。
 * Windows 权限恒为已授权，因此这里不会渲染。
 *
 * 无论缺不缺权限都逐项显示状态并带上「上次检测」时间：
 * 否则用户点了「重新检测」而状态没变时，会以为按钮坏了。
 */
export function PermissionBanner({
  permissions,
  listenerOk,
  onRequest,
  onRefresh,
  onOpenSettings,
  onRestart,
}: PermissionBannerProps) {
  const [checkedAt, setCheckedAt] = useState<Date>(() => new Date());

  // 后端每次推送权限状态（启动申请、手动检测、监听线程每 5 秒重试）都会更新这里
  useEffect(() => {
    setCheckedAt(new Date());
  }, [permissions]);

  if (permissions.platform !== "macos") {
    return null;
  }

  const allGranted =
    permissions.listen_input && permissions.post_events && listenerOk;

  return (
    <div className="permission-banner">
      <div className="permission-banner-head">
        {allGranted ? "权限已就绪" : "需要系统授权后才能录制与回放"}
      </div>

      {items(permissions).map((item) => (
        <div className="permission-item" key={item.kind}>
          <span className="permission-item-text">
            <span className={`permission-flag ${item.granted ? "ok" : "missing"}`}>
              {item.granted ? "已授权" : "未授权"}
            </span>
            <strong>{item.label}</strong>
            <span className="permission-item-detail">
              {item.granted ? item.detail : item.impact}
            </span>
          </span>
          {!item.granted && (
            <button
              className="btn small"
              onClick={() => onOpenSettings(item.kind)}
            >
              去开启
            </button>
          )}
        </div>
      ))}

      <div className="permission-actions">
        <button className="btn small" onClick={onRequest}>
          申请授权
        </button>
        <button className="btn small" onClick={onRefresh}>
          重新检测
        </button>
        <button className="btn small primary" onClick={onRestart}>
          重启应用
        </button>
        <span className="permission-hint">
          上次检测 {formatTime(checkedAt)} · 授权后请点「重启应用」（TCC 要重启进程才生效；
          输入监听未生效时后台每 5 秒自动重试一次）
        </span>
      </div>

      {!allGranted && (
        <div className="permission-hint">
          路径：系统设置 → 隐私与安全性 → 输入监控 / 辅助功能。
          列表里没有本应用时，点「申请授权」触发系统弹窗后它才会出现；
          重新构建过应用（签名变了）需要把旧条目用「−」删掉再重新授权。
        </div>
      )}
    </div>
  );
}