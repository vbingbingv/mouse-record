use std::time::{Duration, Instant};

use crate::model::action::{MouseAction, MouseButton};
use crate::model::recording::RecordedAction;

/// 无按键按下时，普通 MouseMove 的压缩阈值：
/// 距离超过该值，或距上一个已记录事件超过该时间，才记录。
/// 阈值过大会让慢速移动录成稀疏跳点，回放时顿挫感明显。
pub const MOVE_MIN_DISTANCE_PX: f64 = 1.0;
pub const MOVE_MIN_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MouseButtonState {
    pub left: bool,
    pub middle: bool,
    pub right: bool,
    pub back: bool,
    pub forward: bool,
}

impl MouseButtonState {
    pub fn set(&mut self, button: MouseButton, down: bool) {
        match button {
            MouseButton::Left => self.left = down,
            MouseButton::Middle => self.middle = down,
            MouseButton::Right => self.right = down,
            MouseButton::Back => self.back = down,
            MouseButton::Forward => self.forward = down,
        }
    }

    pub fn any_down(&self) -> bool {
        self.left || self.middle || self.right || self.back || self.forward
    }
}

/// 录制工作器：把原始鼠标事件流转换为压缩后的 RecordedAction 序列。
///
/// 不依赖任何第三方 crate，可独立单元测试。
#[derive(Debug, Default)]
pub struct RecordingWorker {
    actions: Vec<RecordedAction>,
    buttons: MouseButtonState,
    last_recorded_at: Option<Instant>,
    last_recorded_move: Option<(f64, f64)>,
    last_seen_pos: Option<(f64, f64)>,
}

impl RecordingWorker {
    pub fn new() -> Self {
        Self::default()
    }

    /// 处理一个到达于 `at` 时刻的原始鼠标事件。
    pub fn handle(&mut self, action: MouseAction, at: Instant) {
        match action {
            MouseAction::Move { x, y } => {
                self.last_seen_pos = Some((x, y));

                // 拖拽状态（任意按键按下）下的移动全部保留，保证轨迹完整。
                if !self.buttons.any_down() && !self.should_keep_move(x, y, at) {
                    return;
                }

                self.record(MouseAction::Move { x, y }, at);
                self.last_recorded_move = Some((x, y));
            }

            MouseAction::ButtonDown { button } => {
                self.buttons.set(button, true);
                self.record(action, at);
            }

            MouseAction::ButtonUp { button } => {
                self.buttons.set(button, false);
                self.record(action, at);
            }

            MouseAction::Wheel { delta_x, delta_y } => {
                if delta_x == 0 && delta_y == 0 {
                    return;
                }
                self.record(action, at);
            }
        }
    }

    fn should_keep_move(&self, x: f64, y: f64, at: Instant) -> bool {
        match self.last_recorded_move {
            None => true,
            Some((lx, ly)) => {
                let distance = (x - lx).hypot(y - ly);
                if distance >= MOVE_MIN_DISTANCE_PX {
                    return true;
                }
                match self.last_recorded_at {
                    None => true,
                    Some(last) => at.saturating_duration_since(last) >= MOVE_MIN_INTERVAL,
                }
            }
        }
    }

    fn record(&mut self, action: MouseAction, at: Instant) {
        // 微秒精度：高频移动事件间隔常小于 1ms，毫秒截断会丢失节奏
        let delay_us = self
            .last_recorded_at
            .map(|last| at.saturating_duration_since(last).as_micros() as u64)
            .unwrap_or(0);

        self.actions.push(RecordedAction { delay_us, action });
        self.last_recorded_at = Some(at);
    }

    pub fn finish(self) -> Vec<RecordedAction> {
        self.actions
    }

    pub fn action_count(&self) -> usize {
        self.actions.len()
    }

    pub fn last_seen_pos(&self) -> Option<(f64, f64)> {
        self.last_seen_pos
    }

    pub fn last_action(&self) -> Option<&RecordedAction> {
        self.actions.last()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_all_event_kinds_with_delay() {
        let mut w = RecordingWorker::new();
        let t0 = Instant::now();

        w.handle(MouseAction::Move { x: 100.0, y: 100.0 }, t0);
        w.handle(
            MouseAction::ButtonDown {
                button: MouseButton::Left,
            },
            t0 + Duration::from_millis(120),
        );
        w.handle(
            MouseAction::Move { x: 120.0, y: 110.0 },
            t0 + Duration::from_millis(140),
        );
        w.handle(
            MouseAction::ButtonUp {
                button: MouseButton::Left,
            },
            t0 + Duration::from_millis(200),
        );
        w.handle(
            MouseAction::Wheel {
                delta_x: 0,
                delta_y: -3,
            },
            t0 + Duration::from_millis(300),
        );

        let actions = w.finish();
        assert_eq!(actions.len(), 5);
        assert_eq!(actions[0].delay_us, 0);
        assert_eq!(actions[1].delay_us, 120_000);
        assert_eq!(actions[2].delay_us, 20_000);
        assert_eq!(actions[3].delay_us, 60_000);
        assert_eq!(actions[4].delay_us, 100_000);
    }

    #[test]
    fn compresses_idle_moves_but_keeps_threshold() {
        let t0 = Instant::now();
        let mut w = RecordingWorker::new();

        w.handle(MouseAction::Move { x: 0.0, y: 0.0 }, t0);

        // 距离 0.4px 且间隔 5ms：被压缩
        w.handle(
            MouseAction::Move { x: 0.4, y: 0.0 },
            t0 + Duration::from_millis(5),
        );

        // 距离 0.4px 但间隔 15ms：保留（时间阈值）
        w.handle(
            MouseAction::Move { x: 0.8, y: 0.0 },
            t0 + Duration::from_millis(15),
        );

        // 距离 1.7px：保留（距离阈值）
        w.handle(
            MouseAction::Move { x: 2.5, y: 0.0 },
            t0 + Duration::from_millis(20),
        );

        let actions = w.finish();
        assert_eq!(actions.len(), 3);
        assert_eq!(actions[0].delay_us, 0);
        assert_eq!(actions[1].delay_us, 15_000);
        assert_eq!(actions[2].delay_us, 5_000);
    }

    #[test]
    fn keeps_sub_millisecond_delay_precision() {
        let t0 = Instant::now();
        let mut w = RecordingWorker::new();

        // 拖拽状态下 250us 间隔的移动也应保留微秒级 delay
        w.handle(
            MouseAction::ButtonDown {
                button: MouseButton::Left,
            },
            t0,
        );
        w.handle(
            MouseAction::Move { x: 1.0, y: 1.0 },
            t0 + Duration::from_micros(250),
        );
        w.handle(
            MouseAction::Move { x: 2.0, y: 2.0 },
            t0 + Duration::from_micros(500),
        );

        let actions = w.finish();
        assert_eq!(actions.len(), 3);
        assert_eq!(actions[1].delay_us, 250);
        assert_eq!(actions[2].delay_us, 250);
    }

    #[test]
    fn keeps_all_moves_while_button_down() {
        let t0 = Instant::now();
        let mut w = RecordingWorker::new();

        w.handle(MouseAction::Move { x: 0.0, y: 0.0 }, t0);
        w.handle(
            MouseAction::ButtonDown {
                button: MouseButton::Left,
            },
            t0 + Duration::from_millis(10),
        );

        // 拖拽中：1px、2ms 的移动也全部保留
        for i in 1..=5 {
            w.handle(
                MouseAction::Move {
                    x: i as f64,
                    y: i as f64,
                },
                t0 + Duration::from_millis(10 + i * 2),
            );
        }

        w.handle(
            MouseAction::ButtonUp {
                button: MouseButton::Left,
            },
            t0 + Duration::from_millis(30),
        );

        let actions = w.finish();
        assert_eq!(actions.len(), 8);
    }

    #[test]
    fn tracks_multiple_simultaneous_buttons() {
        let t0 = Instant::now();
        let mut w = RecordingWorker::new();

        w.handle(
            MouseAction::ButtonDown {
                button: MouseButton::Left,
            },
            t0,
        );
        w.handle(
            MouseAction::ButtonDown {
                button: MouseButton::Right,
            },
            t0 + Duration::from_millis(10),
        );

        // 两个键同时按下：移动不压缩
        w.handle(
            MouseAction::Move { x: 1.0, y: 1.0 },
            t0 + Duration::from_millis(11),
        );
        assert_eq!(w.action_count(), 3);

        w.handle(
            MouseAction::ButtonUp {
                button: MouseButton::Left,
            },
            t0 + Duration::from_millis(20),
        );
        w.handle(
            MouseAction::ButtonUp {
                button: MouseButton::Right,
            },
            t0 + Duration::from_millis(30),
        );

        // 全部释放后恢复压缩
        w.handle(
            MouseAction::Move { x: 1.5, y: 1.5 },
            t0 + Duration::from_millis(31),
        );
        assert_eq!(w.action_count(), 5);
    }

    #[test]
    fn ignores_zero_wheel() {
        let t0 = Instant::now();
        let mut w = RecordingWorker::new();
        w.handle(
            MouseAction::Wheel {
                delta_x: 0,
                delta_y: 0,
            },
            t0,
        );
        assert_eq!(w.action_count(), 0);
    }

    #[test]
    fn button_state_set_and_clear() {
        let mut s = MouseButtonState::default();
        s.set(MouseButton::Middle, true);
        s.set(MouseButton::Back, true);
        assert!(s.any_down());
        s.set(MouseButton::Middle, false);
        s.set(MouseButton::Back, false);
        assert!(!s.any_down());
    }
}
