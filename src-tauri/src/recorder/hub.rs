use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::error::AutomationError;
use crate::model::recording::{RecordedAction, Recording};
use crate::model::state::{AtomicEngineState, EngineState};

use super::worker::RecordingWorker;
use crate::input::source::{EventSink, RawInputEvent, RawMouseEvent};

/// 录制状态统计（低频发送给前端）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct RecordingStats {
    pub duration_ms: u64,
    pub action_count: usize,
    pub mouse_x: Option<f64>,
    pub mouse_y: Option<f64>,
    pub last_action: Option<RecordedAction>,
}

const STATS_EMIT_INTERVAL: Duration = Duration::from_millis(300);

struct RecordSession {
    sender: mpsc::Sender<RawMouseEvent>,
    worker: JoinHandle<Vec<RecordedAction>>,
}

/// 录制枢纽：统一持有引擎状态与当前录制会话，
/// 是常驻输入回调的唯一切入点。
///
/// 状态规则（参见 README 第 9 节）：
/// - Idle -> Recording -> Idle
/// - Idle -> Replaying -> (Stopping) -> Idle
/// 其余迁移一律拒绝。
pub struct RecorderHub {
    state: AtomicEngineState,
    replay_stop: Arc<AtomicBool>,
    listener_running: Arc<AtomicBool>,
    session: Mutex<Option<RecordSession>>,
}

impl RecorderHub {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: AtomicEngineState::new(EngineState::Idle),
            replay_stop: Arc::new(AtomicBool::new(false)),
            listener_running: Arc::new(AtomicBool::new(false)),
            session: Mutex::new(None),
        })
    }

    pub fn state(&self) -> EngineState {
        self.state.load()
    }

    pub fn transition_state(
        &self,
        expect: EngineState,
        to: EngineState,
    ) -> Result<(), EngineState> {
        self.state.transition(expect, to)
    }

    pub fn set_state(&self, value: EngineState) {
        self.state.store(value);
    }

    pub fn replay_stop_flag(&self) -> Arc<AtomicBool> {
        self.replay_stop.clone()
    }

    pub fn listener_running_flag(&self) -> Arc<AtomicBool> {
        self.listener_running.clone()
    }

    pub fn is_listener_running(&self) -> bool {
        self.listener_running.load(Ordering::Acquire)
    }

    /// 开始录制会话。调用前状态必须已切换为 Recording。
    pub fn start_session(
        &self,
        on_stats: impl Fn(RecordingStats) + Send + 'static,
    ) -> Result<(), AutomationError> {
        let (sender, receiver) = mpsc::channel::<RawMouseEvent>();

        let worker = std::thread::Builder::new()
            .name("recorder-worker".into())
            .spawn(move || {
                let mut worker = RecordingWorker::new();
                let started_at = Instant::now();
                let mut last_stats_emit = Instant::now();

                for event in receiver {
                    worker.handle(event.action, event.time);

                    if last_stats_emit.elapsed() >= STATS_EMIT_INTERVAL {
                        last_stats_emit = Instant::now();
                        let (mx, my) = worker
                            .last_seen_pos()
                            .map(|(x, y)| (Some(x), Some(y)))
                            .unwrap_or((None, None));
                        on_stats(RecordingStats {
                            duration_ms: started_at.elapsed().as_millis() as u64,
                            action_count: worker.action_count(),
                            mouse_x: mx,
                            mouse_y: my,
                            last_action: worker.last_action().cloned(),
                        });
                    }
                }

                worker.finish()
            })
            .map_err(|e| AutomationError::InputBackend(format!("spawn recorder worker: {e}")))?;

        let mut session = self
            .session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if session.is_some() {
            return Err(AutomationError::AlreadyRecording);
        }
        *session = Some(RecordSession { sender, worker });
        Ok(())
    }

    /// 结束录制会话并取回完整录制数据。
    /// 调用前状态必须已切换为 Idle（确保回调不再注入新事件）。
    pub fn stop_session(&self) -> Result<Recording, AutomationError> {
        let session = self
            .session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
            .ok_or(AutomationError::NotRecording)?;

        // drop 唯一 sender，worker 处理完积压事件后自然退出
        drop(session.sender);

        let actions = session
            .worker
            .join()
            .map_err(|_| AutomationError::InputBackend("recorder worker panicked".into()))?;

        Ok(Recording::new(actions))
    }

    /// 构造常驻输入回调（策略层）。
    pub fn event_sink(self: &Arc<Self>) -> EventSink {
        let hub = Arc::clone(self);
        Arc::new(move |event: RawInputEvent| match event {
            RawInputEvent::Mouse(event) => {
                if hub.state.load() == EngineState::Recording {
                    // rdev 回调线程内不允许 panic，锁中毒时直接恢复数据继续使用
                    let guard = hub
                        .session
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    if let Some(session) = guard.as_ref() {
                        let _ = session.sender.send(event);
                    }
                }
            }
            RawInputEvent::EscapePressed => {
                if hub.state.load() == EngineState::Replaying {
                    hub.replay_stop.store(true, Ordering::Relaxed);
                }
            }
            RawInputEvent::Other => {}
        })
    }

    /// 回放前的保护：清空可能残留的停止标记。
    pub fn reset_replay_stop(&self) {
        self.replay_stop.store(false, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::action::{MouseAction, MouseButton};

    fn mouse(action: MouseAction) -> RawInputEvent {
        RawInputEvent::Mouse(RawMouseEvent {
            time: Instant::now(),
            action,
        })
    }

    #[test]
    fn records_events_through_sink_and_stops() {
        let hub = RecorderHub::new();

        hub.transition_state(EngineState::Idle, EngineState::Recording)
            .unwrap();
        hub.start_session(|_| {}).unwrap();

        let sink = hub.event_sink();

        // Idle 之外不记录（Replaying 期间忽略）
        hub.transition_state(EngineState::Recording, EngineState::Idle)
            .unwrap();
        hub.transition_state(EngineState::Idle, EngineState::Replaying)
            .unwrap();
        sink(mouse(MouseAction::Move { x: 1.0, y: 1.0 }));
        hub.transition_state(EngineState::Replaying, EngineState::Idle)
            .unwrap();

        // 正常录制
        hub.transition_state(EngineState::Idle, EngineState::Recording)
            .unwrap();
        sink(mouse(MouseAction::Move { x: 10.0, y: 10.0 }));
        std::thread::sleep(Duration::from_millis(5));
        sink(mouse(MouseAction::ButtonDown {
            button: MouseButton::Left,
        }));
        std::thread::sleep(Duration::from_millis(5));
        sink(mouse(MouseAction::ButtonUp {
            button: MouseButton::Left,
        }));

        hub.transition_state(EngineState::Recording, EngineState::Idle)
            .unwrap();
        let recording = hub.stop_session().unwrap();

        assert_eq!(recording.version, 2);
        assert_eq!(recording.actions.len(), 3);
        // sleep 至少 5ms，delay 可能为 5~10ms，只要非负且合理即可
        assert!(recording.actions[1].delay_us <= 100_000);
        assert!(recording.actions[2].delay_us <= 100_000);

        // 停止后再次 stop 报错
        assert!(matches!(
            hub.stop_session(),
            Err(AutomationError::NotRecording)
        ));
    }

    #[test]
    fn escape_sets_replay_stop_only_when_replaying() {
        let hub = RecorderHub::new();

        hub.transition_state(EngineState::Idle, EngineState::Replaying)
            .unwrap();
        hub.reset_replay_stop();
        let sink = hub.event_sink();
        sink(RawInputEvent::EscapePressed);
        assert!(hub.replay_stop_flag().load(Ordering::Relaxed));

        // Idle 下按 ESC 无副作用
        hub.set_state(EngineState::Idle);
        hub.reset_replay_stop();
        sink(RawInputEvent::EscapePressed);
        assert!(!hub.replay_stop_flag().load(Ordering::Relaxed));
    }

    #[test]
    fn rejects_concurrent_sessions() {
        let hub = RecorderHub::new();
        hub.set_state(EngineState::Recording);
        hub.start_session(|_| {}).unwrap();
        assert!(matches!(
            hub.start_session(|_| {}),
            Err(AutomationError::AlreadyRecording)
        ));

        // 清理
        hub.set_state(EngineState::Idle);
        let _ = hub.stop_session().unwrap();
    }
}
