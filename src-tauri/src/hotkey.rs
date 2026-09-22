use std::sync::mpsc;
use std::sync::Arc;

use tauri::{AppHandle, Emitter};

use crate::commands::recording::{start_recording_impl, stop_recording_impl};
use crate::commands::replay::{start_replay_impl, stop_replay_impl};
use crate::error::AutomationError;
use crate::input::source::Hotkey;
use crate::model::state::EngineState;
use crate::AppState;

/// 热键在当前引擎状态下的动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToggleDecision {
    Start,
    Stop,
    Ignore,
}

/// 纯决策逻辑：Idle 下开始，对应活动态下结束，
/// 其余（含 Stopping 与交叉状态）一律忽略，不做隐式串联。
pub fn decide(state: EngineState, hotkey: Hotkey) -> ToggleDecision {
    match (hotkey, state) {
        (Hotkey::ToggleRecording, EngineState::Idle) => ToggleDecision::Start,
        (Hotkey::ToggleRecording, EngineState::Recording) => ToggleDecision::Stop,
        (Hotkey::ToggleReplay, EngineState::Idle) => ToggleDecision::Start,
        (Hotkey::ToggleReplay, EngineState::Replaying) => ToggleDecision::Stop,
        _ => ToggleDecision::Ignore,
    }
}

/// 热键分发器：把输入钩子里的热键事件搬到独立线程串行执行。
///
/// 钩子回调必须尽快返回（超过 LowLevelHooksTimeout 会被系统静默摘掉钩子），
/// 而结束录制要 join 录制 worker、结束回放要等回放线程收尾，都不能在回调线程里做；
/// 单线程消费同时保证 toggle 串行，不会因为连点而叠加状态迁移。
pub struct HotkeyDispatcher {
    tx: mpsc::Sender<Hotkey>,
}

impl HotkeyDispatcher {
    pub fn new(app: AppHandle, state: AppState) -> Arc<Self> {
        let (tx, rx) = mpsc::channel::<Hotkey>();

        std::thread::Builder::new()
            .name("hotkey-actions".into())
            .spawn(move || {
                for hotkey in rx {
                    if let Err(e) = handle(&app, &state, hotkey) {
                        let _ = app.emit("hotkey-error", e.to_string());
                    }
                }
            })
            .expect("failed to spawn hotkey dispatcher thread");

        Arc::new(Self { tx })
    }

    /// 非阻塞投递（在输入钩子回调线程内调用）。
    pub fn dispatch(&self, hotkey: Hotkey) {
        let _ = self.tx.send(hotkey);
    }
}

fn handle(app: &AppHandle, state: &AppState, hotkey: Hotkey) -> Result<(), AutomationError> {
    match decide(state.hub.state(), hotkey) {
        ToggleDecision::Ignore => Ok(()),
        ToggleDecision::Start => match hotkey {
            Hotkey::ToggleRecording => start_recording_impl(app, state),
            Hotkey::ToggleReplay => start_replay_from_current(app, state),
        },
        ToggleDecision::Stop => match hotkey {
            Hotkey::ToggleRecording => stop_recording_impl(app, state).map(|_| ()),
            Hotkey::ToggleReplay => stop_replay_impl(state),
        },
    }
}

/// Ctrl+9：回放当前持有的录制，沿用界面上设置的回放选项。
/// （窗口隐藏期间无法再改设置，所以前端会提前把参数推给后端。）
fn start_replay_from_current(app: &AppHandle, state: &AppState) -> Result<(), AutomationError> {
    // 分发线程是常驻的，锁中毒时不能 panic 掉它
    let recording = state
        .current_recording
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
        .ok_or_else(|| {
            AutomationError::InvalidRecording("没有可回放的录制，请先录制或加载一段录制".into())
        })?;

    let options = state
        .last_replay_options
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();

    start_replay_impl(app, state, recording, options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_starts_the_matching_feature() {
        assert_eq!(
            decide(EngineState::Idle, Hotkey::ToggleRecording),
            ToggleDecision::Start
        );
        assert_eq!(
            decide(EngineState::Idle, Hotkey::ToggleReplay),
            ToggleDecision::Start
        );
    }

    #[test]
    fn active_state_stops_the_matching_feature() {
        assert_eq!(
            decide(EngineState::Recording, Hotkey::ToggleRecording),
            ToggleDecision::Stop
        );
        assert_eq!(
            decide(EngineState::Replaying, Hotkey::ToggleReplay),
            ToggleDecision::Stop
        );
    }

    #[test]
    fn cross_and_transient_states_are_ignored() {
        assert_eq!(
            decide(EngineState::Recording, Hotkey::ToggleReplay),
            ToggleDecision::Ignore
        );
        assert_eq!(
            decide(EngineState::Replaying, Hotkey::ToggleRecording),
            ToggleDecision::Ignore
        );
        assert_eq!(
            decide(EngineState::Stopping, Hotkey::ToggleRecording),
            ToggleDecision::Ignore
        );
        assert_eq!(
            decide(EngineState::Stopping, Hotkey::ToggleReplay),
            ToggleDecision::Ignore
        );
    }
}
