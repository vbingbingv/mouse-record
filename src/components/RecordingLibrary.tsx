import { useState } from "react";

import type { RecordingMeta } from "../types/automation";
import { formatDuration } from "./StatusBar";

interface RecordingLibraryProps {
  metas: RecordingMeta[];
  canSave: boolean;
  onSave: (name: string) => Promise<void>;
  onLoad: (name: string) => Promise<void>;
  onDelete: (name: string) => Promise<void>;
}

export function RecordingLibrary({
  metas,
  canSave,
  onSave,
  onLoad,
  onDelete,
}: RecordingLibraryProps) {
  const [name, setName] = useState("");

  const save = async () => {
    if (!name.trim()) return;
    await onSave(name);
    setName("");
  };

  return (
    <section className="panel">
      <h2>录制库</h2>

      <div className="form-row">
        <input
          className="name-input"
          type="text"
          placeholder="录制名称"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void save();
          }}
        />
        <button className="btn" disabled={!canSave || !name.trim()} onClick={save}>
          保存当前录制
        </button>
      </div>

      {metas.length === 0 ? (
        <div className="timeline-empty">暂无已保存的录制</div>
      ) : (
        <div className="library">
          {metas.map((meta) => (
            <div className="library-item" key={meta.name}>
              <div className="library-info">
                <span className="library-name">{meta.name}</span>
                <span className="library-meta">
                  {meta.action_count} 条 · {formatDuration(meta.duration_ms)}
                </span>
              </div>
              <div className="library-actions">
                <button className="btn small" onClick={() => onLoad(meta.name)}>
                  加载
                </button>
                <button
                  className="btn small danger"
                  onClick={() => onDelete(meta.name)}
                >
                  删除
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
