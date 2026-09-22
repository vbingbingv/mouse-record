import { useCallback, useEffect, useState } from "react";

import { ActionTimeline } from "./components/ActionTimeline";
import { RecordControls } from "./components/RecordControls";
import { RecordingLibrary } from "./components/RecordingLibrary";
import { ReplayControls } from "./components/ReplayControls";
import { StatusBar } from "./components/StatusBar";
import { useAutomation } from "./hooks/useAutomation";
import {
  deleteRecording as deleteRecordingApi,
  listRecordings,
  loadRecording,
  saveRecording,
} from "./api/automation";
import type { RecordingMeta } from "./types/automation";

export default function App() {
  const auto = useAutomation();
  const [metas, setMetas] = useState<RecordingMeta[]>([]);

  const refreshMetas = useCallback(async () => {
    try {
      setMetas(await listRecordings());
    } catch (e) {
      console.error("list recordings failed:", e);
    }
  }, []);

  useEffect(() => {
    void refreshMetas();
  }, [refreshMetas]);

  const handleSave = useCallback(
    async (name: string) => {
      try {
        await saveRecording(name);
        await refreshMetas();
      } catch (e) {
        console.error(e);
      }
    },
    [refreshMetas],
  );

  const handleLoad = useCallback(
    async (name: string) => {
      try {
        const recording = await loadRecording(name);
        auto.adoptRecording(recording);
      } catch (e) {
        console.error(e);
      }
    },
    [auto],
  );

  const handleDelete = useCallback(
    async (name: string) => {
      try {
        await deleteRecordingApi(name);
        await refreshMetas();
      } catch (e) {
        console.error(e);
      }
    },
    [refreshMetas],
  );

  return (
    <div className="app">
      <header className="app-header">
        <h1>Mouse Record</h1>
        <StatusBar
          engineState={auto.engineState}
          listenerOk={auto.listenerOk}
        />
      </header>

      {auto.error && (
        <div className="error-toast" onClick={auto.clearError}>
          {auto.error}
          <span className="error-close">点击关闭</span>
        </div>
      )}

      <main className="app-main">
        <div className="column">
          <RecordControls
            engineState={auto.engineState}
            stats={auto.stats}
            onStart={() => void auto.startRecording()}
            onStop={() => void auto.stopRecording()}
          />

          <ReplayControls
            engineState={auto.engineState}
            recording={auto.recording}
            progress={auto.progress}
            onStart={(options) => void auto.startReplay(options)}
            onStop={() => void auto.stopReplay()}
            onOptionsChange={auto.syncReplayOptions}
          />

          <RecordingLibrary
            metas={metas}
            canSave={auto.recording !== null && auto.engineState === "idle"}
            onSave={handleSave}
            onLoad={handleLoad}
            onDelete={handleDelete}
          />
        </div>

        <div className="column">
          <ActionTimeline recording={auto.recording} />
        </div>
      </main>

      <footer className="app-footer">
        Ctrl+8 开始/结束录制 · Ctrl+9 开始/结束回放 ·
        录制或回放时窗口会自动隐藏，结束后恢复 ·{" "}
        {auto.recording ? "已就绪可回放" : "请先录制或加载一段录制"}
      </footer>
    </div>
  );
}
