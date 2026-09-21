use serde::{Deserialize, Serialize};

use crate::error::AutomationError;

use super::action::MouseAction;

pub const RECORDING_VERSION: u32 = 2;

/// v1 使用毫秒精度的 delay_ms；v2 起改为微秒精度的 delay_us。
pub const LEGACY_RECORDING_VERSION: u32 = 1;

/// 距离上一个已记录 Action 的时间以微秒保存：
/// 高频鼠标（拖拽可达 125~1000Hz）事件的间隔常小于 1ms，
/// 毫秒截断会把它们全部记成 0，导致回放时连发跳帧。
#[derive(Debug, Clone, Serialize)]
pub struct RecordedAction {
    pub delay_us: u64,

    pub action: MouseAction,
}

impl<'de> Deserialize<'de> for RecordedAction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawRecordedAction {
            #[serde(default)]
            delay_us: Option<u64>,

            /// v1 兼容：无 delay_us 时按 delay_ms 换算
            #[serde(default)]
            delay_ms: Option<u64>,

            action: MouseAction,
        }

        let raw = RawRecordedAction::deserialize(deserializer)?;
        let delay_us = raw
            .delay_us
            .or_else(|| raw.delay_ms.map(|ms| ms.saturating_mul(1000)))
            .ok_or_else(|| serde::de::Error::missing_field("delay_us"))?;

        Ok(RecordedAction {
            delay_us,
            action: raw.action,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recording {
    pub version: u32,

    pub actions: Vec<RecordedAction>,
}

impl Recording {
    pub fn new(actions: Vec<RecordedAction>) -> Self {
        Self {
            version: RECORDING_VERSION,
            actions,
        }
    }

    pub fn total_duration_us(&self) -> u64 {
        self.actions.iter().map(|a| a.delay_us).sum()
    }

    pub fn total_duration_ms(&self) -> u64 {
        self.total_duration_us() / 1000
    }

    pub fn validate(&self) -> Result<(), AutomationError> {
        if self.version != RECORDING_VERSION && self.version != LEGACY_RECORDING_VERSION {
            return Err(AutomationError::InvalidRecording(format!(
                "unsupported version {} (expected {})",
                self.version, RECORDING_VERSION
            )));
        }
        Ok(())
    }

    /// v1 -> v2 迁移（delay_ms 已在反序列化时换算为 delay_us）。
    pub fn migrate_legacy(&mut self) {
        if self.version == LEGACY_RECORDING_VERSION {
            self.version = RECORDING_VERSION;
        }
    }
}

fn default_loops() -> u32 {
    1
}

fn default_speed() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayOptions {
    #[serde(default = "default_loops")]
    pub loops: u32,

    #[serde(default)]
    pub loop_interval_ms: u64,

    #[serde(default = "default_speed")]
    pub speed: f64,

    /// 无限循环：为 true 时忽略 loops，一直回放到被停止为止。
    #[serde(default)]
    pub infinite: bool,
}

impl Default for ReplayOptions {
    fn default() -> Self {
        Self {
            loops: default_loops(),
            loop_interval_ms: 0,
            speed: default_speed(),
            infinite: false,
        }
    }
}

impl ReplayOptions {
    pub fn validate(&self) -> Result<(), AutomationError> {
        if !self.infinite && self.loops == 0 {
            return Err(AutomationError::InvalidOptions("loops must be >= 1".into()));
        }
        if !(0.1..=10.0).contains(&self.speed) {
            return Err(AutomationError::InvalidOptions(
                "speed must be within 0.1..=10.0".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::action::MouseButton;

    #[test]
    fn serializes_v2_shape_with_delay_us() {
        let recording = Recording {
            version: 2,
            actions: vec![RecordedAction {
                delay_us: 200_000,
                action: MouseAction::ButtonDown {
                    button: MouseButton::Left,
                },
            }],
        };
        let json = serde_json::to_string_pretty(&recording).unwrap();
        assert!(json.contains("\"version\": 2"), "{json}");
        assert!(json.contains("\"delay_us\": 200000"), "{json}");
        assert!(json.contains(r#""button": "Left""#), "{json}");
    }

    #[test]
    fn roundtrip() {
        let recording = Recording::new(vec![
            RecordedAction {
                delay_us: 120_500,
                action: MouseAction::Wheel {
                    delta_x: 0,
                    delta_y: 2,
                },
            },
            RecordedAction {
                delay_us: 800,
                action: MouseAction::ButtonUp {
                    button: MouseButton::Right,
                },
            },
        ]);
        let json = serde_json::to_string(&recording).unwrap();
        let parsed: Recording = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, 2);
        assert_eq!(parsed.actions.len(), 2);
        assert_eq!(parsed.actions[0].delay_us, 120_500);
        assert_eq!(parsed.actions[1].delay_us, 800);
        assert_eq!(parsed.total_duration_ms(), 121);
    }

    #[test]
    fn deserializes_legacy_v1_delay_ms() {
        let json = r#"{
            "version": 1,
            "actions": [
                { "delay_ms": 0, "action": { "type": "Move", "x": 500, "y": 400 } },
                { "delay_ms": 200, "action": { "type": "ButtonDown", "button": "Left" } }
            ]
        }"#;
        let mut recording: Recording = serde_json::from_str(json).unwrap();
        assert_eq!(recording.version, 1);
        assert!(recording.validate().is_ok());
        assert_eq!(recording.actions[1].delay_us, 200_000);

        recording.migrate_legacy();
        assert_eq!(recording.version, RECORDING_VERSION);
    }

    #[test]
    fn missing_delay_field_is_rejected() {
        let json = r#"{ "action": { "type": "Move", "x": 1, "y": 2 } }"#;
        assert!(serde_json::from_str::<RecordedAction>(json).is_err());
    }

    #[test]
    fn rejects_unsupported_version() {
        let recording = Recording {
            version: 99,
            actions: vec![],
        };
        assert!(recording.validate().is_err());
    }

    #[test]
    fn replay_options_defaults() {
        let opts: ReplayOptions = serde_json::from_str("{}").unwrap();
        assert_eq!(opts.loops, 1);
        assert_eq!(opts.loop_interval_ms, 0);
        assert_eq!(opts.speed, 1.0);
        assert!(!opts.infinite);
        assert!(opts.validate().is_ok());
    }

    #[test]
    fn replay_options_rejects_bad_speed() {
        let opts = ReplayOptions {
            loops: 1,
            speed: 0.0,
            ..Default::default()
        };
        assert!(opts.validate().is_err());
    }

    #[test]
    fn replay_options_infinite_skips_loops_check() {
        let opts = ReplayOptions {
            loops: 0,
            infinite: true,
            ..Default::default()
        };
        assert!(opts.validate().is_ok());
    }

    #[test]
    fn replay_options_rejects_zero_loops_when_finite() {
        let opts = ReplayOptions {
            loops: 0,
            infinite: false,
            ..Default::default()
        };
        assert!(opts.validate().is_err());
    }
}
