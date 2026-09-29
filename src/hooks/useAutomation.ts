import { useCallback, useEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

import * as api from "../api/automation";
import type {
  EngineState,
  PermissionKind,
  PermissionStatus,
  Recording,
  RecordingStats,
  ReplayOptions,
  ReplayProgress,
} from "../types/automation";

export interface UseAutomation {
  engineState: EngineState;
  listenerOk: boolean;
  listenerError: string | null;
  permissions: PermissionStatus | null;
  stats: RecordingStats | null;
  recording: Recording | null;
  progress: ReplayProgress | null;
  error: string | null;
  clearError: () => void;
  startRecording: () => Promise<void>;
  stopRecording: () => Promise<void>;
  startReplay: (options: ReplayOptions) => Promise<void>;
  stopReplay: () => Promise<void>;
  syncReplayOptions: (options: ReplayOptions) => void;
  adoptRecording: (recording: Recording) => void;
  refreshPermissions: () => Promise<void>;
  requestPermission: () => Promise<void>;
  openPermissionSettings: (kind: PermissionKind) => Promise<void>;
  restartApp: () => Promise<void>;
}

export function useAutomation(): UseAutomation {
  const [engineState, setEngineState] = useState<EngineState>("idle");
  const [listenerOk, setListenerOk] = useState(true);
  const [listenerError, setListenerError] = useState<string | null>(null);
  const [permissions, setPermissions] = useState<PermissionStatus | null>(null);
  const [stats, setStats] = useState<RecordingStats | null>(null);
  const [recording, setRecording] = useState<Recording | null>(null);
  const [progress, setProgress] = useState<ReplayProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const errorTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const showError = useCallback((message: string) => {
    setError(message);
    if (errorTimer.current) clearTimeout(errorTimer.current);
    errorTimer.current = setTimeout(() => setError(null), 6000);
  }, []);

  useEffect(() => {
    return () => {
      if (errorTimer.current) clearTimeout(errorTimer.current);
    };
  }, []);

  useEffect(() => {
    const unlisteners: Promise<UnlistenFn>[] = [];

    unlisteners.push(
      listen<EngineState>("engine-state-changed", (e) => {
        setEngineState(e.payload);
      }),
    );

    unlisteners.push(
      listen<RecordingStats>("recording-stats", (e) => {
        setStats(e.payload);
      }),
    );

    unlisteners.push(
      listen<Recording>("recording-stopped", (e) => {
        setRecording(e.payload);
        setStats(null);
      }),
    );

    unlisteners.push(
      listen<ReplayProgress>("replay-progress", (e) => {
        setProgress(e.payload);
      }),
    );

    unlisteners.push(
      listen("replay-finished", () => {
        setProgress(null);
      }),
    );

    unlisteners.push(
      listen("replay-stopped", () => {
        setProgress(null);
      }),
    );

    unlisteners.push(
      listen<string>("replay-error", (e) => {
        setProgress(null);
        showError(`回放失败：${e.payload}`);
      }),
    );

    unlisteners.push(
      listen<boolean>("input-listener-status", (e) => {
        if (e.payload) {
          setListenerOk(true);
          setListenerError(null);
        } else {
          setListenerOk(false);
        }
      }),
    );

    unlisteners.push(
      listen<string>("input-listener-error", (e) => {
        setListenerOk(false);
        setListenerError(e.payload);
      }),
    );

    unlisteners.push(
      listen<string>("hotkey-error", (e) => {
        showError(`快捷键操作失败：${e.payload}`);
      }),
    );

    // 后端在启动申请授权、以及监听线程每轮重试后都会推送权限状态
    unlisteners.push(
      listen<PermissionStatus>("permission-status", (e) => {
        setPermissions(e.payload);
      }),
    );

    api
      .getEngineState()
      .then(setEngineState)
      .catch(() => undefined);
    api
      .getListenerStatus()
      .then(setListenerOk)
      .catch(() => undefined);
    api
      .getPermissionStatus()
      .then(setPermissions)
      .catch(() => undefined);

    return () => {
      for (const unlisten of unlisteners) {
        unlisten.then((f) => f()).catch(() => undefined);
      }
    };
  }, [showError]);

  const startRecording = useCallback(async () => {
    try {
      setStats({
        duration_ms: 0,
        action_count: 0,
        mouse_x: null,
        mouse_y: null,
        last_action: null,
      });
      await api.startRecording();
    } catch (e) {
      showError(String(e));
    }
  }, [showError]);

  const stopRecording = useCallback(async () => {
    try {
      await api.stopRecording();
    } catch (e) {
      showError(String(e));
    }
  }, [showError]);

  const startReplay = useCallback(
    async (options: ReplayOptions) => {
      if (!recording) return;
      try {
        await api.startReplay(recording, options);
      } catch (e) {
        showError(String(e));
      }
    },
    [recording, showError],
  );

  const stopReplay = useCallback(async () => {
    try {
      await api.stopReplay();
    } catch (e) {
      showError(String(e));
    }
  }, [showError]);

  // 只用于让后端记住参数，供热键回放使用；失败不必打扰用户（下次改动还会重推）
  const syncReplayOptions = useCallback((options: ReplayOptions) => {
    void api.setReplayOptions(options).catch(() => undefined);
  }, []);

  const adoptRecording = useCallback((next: Recording) => {
    setRecording(next);
    setProgress(null);
  }, []);

  /** 重新检测：授权后系统设置回到应用时用，不必重启。 */
  const refreshPermissions = useCallback(async () => {
    try {
      setPermissions(await api.getPermissionStatus());
    } catch (e) {
      showError(String(e));
    }
  }, [showError]);

  const requestPermission = useCallback(async () => {
    try {
      setPermissions(await api.requestPermission());
    } catch (e) {
      showError(String(e));
    }
  }, [showError]);

  const openPermissionSettings = useCallback(
    async (kind: PermissionKind) => {
      try {
        await api.openPermissionSettings(kind);
      } catch (e) {
        showError(String(e));
      }
    },
    [showError],
  );

  const restartApp = useCallback(async () => {
    try {
      await api.restartApp();
    } catch (e) {
      showError(String(e));
    }
  }, [showError]);

  // 从系统设置切回本应用时自动重新检测，省掉一次「重新检测」点击
  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    let disposed = false;

    void getCurrentWindow()
      .onFocusChanged(({ payload: focused }) => {
        if (focused) void refreshPermissions();
      })
      .then((fn) => {
        if (disposed) {
          fn();
          return;
        }
        unlisten = fn;
      })
      .catch(() => undefined);

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [refreshPermissions]);

  const clearError = useCallback(() => {
    setError(null);
  }, []);

  return {
    engineState,
    listenerOk,
    listenerError,
    permissions,
    stats,
    recording,
    progress,
    error,
    clearError,
    startRecording,
    stopRecording,
    startReplay,
    stopReplay,
    syncReplayOptions,
    adoptRecording,
    refreshPermissions,
    requestPermission,
    openPermissionSettings,
    restartApp,
  };
}
