use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineState {
    Idle,
    Recording,
    Replaying,
    Stopping,
}

impl EngineState {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => EngineState::Recording,
            2 => EngineState::Replaying,
            3 => EngineState::Stopping,
            _ => EngineState::Idle,
        }
    }
}

/// 高频回调里读取状态用的原子包装。
#[derive(Debug)]
pub struct AtomicEngineState(AtomicU8);

impl AtomicEngineState {
    pub fn new(value: EngineState) -> Self {
        Self(AtomicU8::new(value as u8))
    }

    pub fn load(&self) -> EngineState {
        EngineState::from_u8(self.0.load(Ordering::Acquire))
    }

    pub fn store(&self, value: EngineState) {
        self.0.store(value as u8, Ordering::Release);
    }

    /// 仅当当前状态等于 `expect` 时切换到 `new`，
    /// 成功返回 Ok(())，失败返回 Err(实际状态)。
    pub fn transition(&self, expect: EngineState, new: EngineState) -> Result<(), EngineState> {
        match self
            .0
            .compare_exchange(expect as u8, new as u8, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => Ok(()),
            Err(actual) => Err(EngineState::from_u8(actual)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_follows_rules() {
        let state = AtomicEngineState::new(EngineState::Idle);

        // Idle -> Recording
        assert!(state
            .transition(EngineState::Idle, EngineState::Recording)
            .is_ok());
        assert_eq!(state.load(), EngineState::Recording);

        // 重复 Idle -> Recording 被拒绝（已是 Recording）
        assert_eq!(
            state.transition(EngineState::Idle, EngineState::Recording),
            Err(EngineState::Recording)
        );

        // Recording -> Idle
        assert!(state
            .transition(EngineState::Recording, EngineState::Idle)
            .is_ok());
        assert_eq!(state.load(), EngineState::Idle);

        // Idle -> Replaying -> Idle
        assert!(state
            .transition(EngineState::Idle, EngineState::Replaying)
            .is_ok());
        assert!(state
            .transition(EngineState::Replaying, EngineState::Idle)
            .is_ok());
        assert_eq!(state.load(), EngineState::Idle);
    }

    #[test]
    fn serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&EngineState::Idle).unwrap(),
            "\"idle\""
        );
        assert_eq!(
            serde_json::to_string(&EngineState::Recording).unwrap(),
            "\"recording\""
        );
    }
}
