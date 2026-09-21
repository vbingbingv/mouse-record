import { useMemo } from "react";

import type {
  MouseAction,
  MouseButton,
  Recording,
  RecordedAction,
} from "../types/automation";
import { formatDuration } from "./StatusBar";

const BUTTON_LABEL: Record<MouseButton, string> = {
  Left: "左键",
  Middle: "中键",
  Right: "右键",
  Back: "后退键",
  Forward: "前进键",
};

const MAX_RENDERED = 500;

interface TimelineEntry {
  time: string;
  label: string;
  detail: string;
}

function describeAction(action: MouseAction): { label: string; detail: string } {
  switch (action.type) {
    case "Move":
      return {
        label: "移动",
        detail: `(${Math.round(action.x)}, ${Math.round(action.y)})`,
      };
    case "ButtonDown":
      return { label: `按下${BUTTON_LABEL[action.button]}`, detail: "" };
    case "ButtonUp":
      return { label: `释放${BUTTON_LABEL[action.button]}`, detail: "" };
    case "Wheel": {
      const parts: string[] = [];
      if (action.delta_y !== 0) {
        parts.push(
          `${action.delta_y > 0 ? "↓" : "↑"} ${Math.abs(action.delta_y)}`,
        );
      }
      if (action.delta_x !== 0) {
        parts.push(
          `${action.delta_x > 0 ? "→" : "←"} ${Math.abs(action.delta_x)}`,
        );
      }
      return { label: "滚轮", detail: parts.join("  ") };
    }
  }
}

function toEntries(actions: RecordedAction[]): TimelineEntry[] {
  const entries: TimelineEntry[] = [];
  let elapsedUs = 0;

  for (const item of actions) {
    elapsedUs += item.delay_us;
    const { label, detail } = describeAction(item.action);
    entries.push({
      time: formatDuration(Math.round(elapsedUs / 1000)),
      label,
      detail,
    });
  }

  return entries;
}

interface ActionTimelineProps {
  recording: Recording | null;
}

export function ActionTimeline({ recording }: ActionTimelineProps) {
  const entries = useMemo(
    () => (recording ? toEntries(recording.actions) : []),
    [recording],
  );

  const total = recording ? recording.actions.length : 0;
  const shown = entries.slice(0, MAX_RENDERED);

  return (
    <section className="panel timeline-panel">
      <h2>动作时间线{total > 0 ? `（共 ${total} 条）` : ""}</h2>

      {total === 0 ? (
        <div className="timeline-empty">暂无录制内容</div>
      ) : (
        <div className="timeline">
          {shown.map((entry, index) => (
            <div className="timeline-item" key={index}>
              <span className="timeline-time">{entry.time}</span>
              <span className="timeline-label">{entry.label}</span>
              <span className="timeline-detail">{entry.detail}</span>
            </div>
          ))}
          {total > shown.length && (
            <div className="timeline-more">
              … 其余 {total - shown.length} 条已省略
            </div>
          )}
        </div>
      )}
    </section>
  );
}
