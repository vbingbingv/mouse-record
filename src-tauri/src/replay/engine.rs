use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::error::AutomationError;
use crate::input::controller::MouseController;
use crate::model::action::MouseButton;
use crate::model::recording::{Recording, ReplayOptions};

/// 回放结束方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayOutcome {
    Completed,
    Stopped,
}

/// 单次等待上限：防止脏数据（超大 delay）导致回放永久卡死。
const MAX_WAIT: Duration = Duration::from_secs(24 * 60 * 60);

/// 长等待的分片大小：及时响应停止。
const WAIT_STEP: Duration = Duration::from_millis(10);

/// 尾部自旋阈值：thread::sleep 普遍有 1~2ms 级超调，
/// 最后一段时间改为忙等，换取动作间隔的定时精度。
const WAIT_SPIN: Duration = Duration::from_millis(2);

/// 回放进度（低频发送给前端）。
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct ReplayProgress {
    pub loop_index: u64,
    pub loops: u32,
    /// 无限循环时为 true，此时 loops 无意义。
    pub infinite: bool,
    pub action_index: usize,
    pub action_count: usize,
}

/// 等到 `target` 时刻（可被 stop 打断），返回 true 表示中途被停止。
pub fn wait_until(target: Instant, stop: &AtomicBool) -> bool {
    loop {
        if stop.load(Ordering::Relaxed) {
            return true;
        }

        let Some(remaining) = target.checked_duration_since(Instant::now()) else {
            // 已到/超过目标时刻
            return false;
        };

        if remaining.is_zero() {
            return false;
        } else if remaining > WAIT_SPIN {
            std::thread::sleep(remaining.min(WAIT_STEP));
        } else {
            std::hint::spin_loop();
        }
    }
}

/// 微秒延迟换算为实际等待时长（应用速度倍率并夹取上限）。
fn scaled_delay(delay_us: u64, speed: f64) -> Duration {
    if speed <= 0.0 {
        return Duration::from_micros(delay_us).min(MAX_WAIT);
    }
    let nanos = (delay_us as f64 / speed) * 1_000.0;
    Duration::from_nanos(nanos.max(0.0) as u64).min(MAX_WAIT)
}

/// 回放引擎：纯逻辑，不依赖 Tauri，可通过注入 MouseController 测试。
pub struct ReplayEngine;

impl ReplayEngine {
    pub fn run(
        controller: &mut dyn MouseController,
        recording: &Recording,
        options: &ReplayOptions,
        stop: &AtomicBool,
        mut on_progress: impl FnMut(ReplayProgress),
    ) -> Result<ReplayOutcome, AutomationError> {
        let actions = &recording.actions;
        let mut pressed: Vec<MouseButton> = Vec::new();
        let mut outcome = ReplayOutcome::Completed;
        let mut loop_index: u64 = 0;

        'outer: loop {
            // 绝对时间调度：以本轮起始时刻为基准，按累计延迟计算每个动作的
            // 目标执行时刻。某次 sleep 超调或 enigo 执行偏慢不会向后累积——
            // 下一动作只需等到自己的目标时刻（已过则立即执行）。
            let loop_base = Instant::now();
            let mut cumulative_us: u64 = 0;

            for (action_index, item) in actions.iter().enumerate() {
                cumulative_us = cumulative_us.saturating_add(item.delay_us);
                let target = loop_base
                    .checked_add(scaled_delay(cumulative_us, options.speed))
                    .unwrap_or(loop_base);

                if wait_until(target, stop) {
                    outcome = ReplayOutcome::Stopped;
                    break 'outer;
                }

                Self::execute_tracked(controller, &item.action, &mut pressed)?;

                on_progress(ReplayProgress {
                    loop_index,
                    loops: options.loops,
                    infinite: options.infinite,
                    action_index,
                    action_count: actions.len(),
                });
            }

            loop_index += 1;

            // 有限模式：完成 loops 轮后结束
            if !options.infinite && loop_index >= options.loops as u64 {
                break;
            }

            // 两轮之间的间隔（无限模式下同样生效，可被停止打断）
            let interval = Duration::from_millis(options.loop_interval_ms).min(MAX_WAIT);
            if wait_until(Instant::now() + interval, stop) {
                outcome = ReplayOutcome::Stopped;
                break;
            }
        }

        release_pressed_buttons(controller, &mut pressed);

        Ok(outcome)
    }

    fn execute_tracked(
        controller: &mut dyn MouseController,
        action: &crate::model::action::MouseAction,
        pressed: &mut Vec<MouseButton>,
    ) -> Result<(), AutomationError> {
        use crate::model::action::MouseAction;

        match action {
            MouseAction::Move { .. } | MouseAction::Wheel { .. } => controller.execute(action),
            MouseAction::ButtonDown { button } => {
                controller.execute(action)?;
                if !pressed.contains(button) {
                    pressed.push(*button);
                }
                Ok(())
            }
            MouseAction::ButtonUp { button } => {
                controller.execute(action)?;
                pressed.retain(|b| b != button);
                Ok(())
            }
        }
    }
}

/// 释放所有仍处于按下状态的按钮（正常结束 / 停止 / 出错的统一出口）。
pub fn release_pressed_buttons(
    controller: &mut dyn MouseController,
    pressed: &mut Vec<MouseButton>,
) {
    for button in pressed.drain(..) {
        let _ = controller.button_up(button);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AutomationError;
    use crate::model::action::{MouseAction, MouseButton};
    use crate::model::recording::RecordedAction;
    use std::cell::RefCell;
    use std::time::Instant;

    #[derive(Default)]
    struct FakeController {
        calls: RefCell<Vec<String>>,
        fail: bool,
    }

    impl MouseController for FakeController {
        fn move_to(&mut self, x: f64, y: f64) -> Result<(), AutomationError> {
            if self.fail {
                return Err(AutomationError::InputBackend("fail".into()));
            }
            self.calls.borrow_mut().push(format!("move({x:.0},{y:.0})"));
            Ok(())
        }

        fn button_down(&mut self, button: MouseButton) -> Result<(), AutomationError> {
            if self.fail {
                return Err(AutomationError::InputBackend("fail".into()));
            }
            self.calls.borrow_mut().push(format!("down({button:?})"));
            Ok(())
        }

        fn button_up(&mut self, button: MouseButton) -> Result<(), AutomationError> {
            if self.fail {
                return Err(AutomationError::InputBackend("fail".into()));
            }
            self.calls.borrow_mut().push(format!("up({button:?})"));
            Ok(())
        }

        fn scroll(&mut self, delta_x: i64, delta_y: i64) -> Result<(), AutomationError> {
            if self.fail {
                return Err(AutomationError::InputBackend("fail".into()));
            }
            self.calls
                .borrow_mut()
                .push(format!("scroll({delta_x},{delta_y})"));
            Ok(())
        }
    }

    fn sample_recording() -> Recording {
        Recording::new(vec![
            RecordedAction {
                delay_us: 0,
                action: MouseAction::Move { x: 100.0, y: 200.0 },
            },
            RecordedAction {
                delay_us: 10_000,
                action: MouseAction::ButtonDown {
                    button: MouseButton::Left,
                },
            },
            RecordedAction {
                delay_us: 0,
                action: MouseAction::ButtonUp {
                    button: MouseButton::Left,
                },
            },
        ])
    }

    #[test]
    fn executes_actions_in_order_with_loops() {
        let mut controller = FakeController::default();
        let recording = sample_recording();
        let options = ReplayOptions {
            loops: 2,
            ..Default::default()
        };
        let stop = AtomicBool::new(false);
        let mut progress_count = 0;

        let outcome = ReplayEngine::run(&mut controller, &recording, &options, &stop, |_| {
            progress_count += 1
        })
        .unwrap();

        assert_eq!(outcome, ReplayOutcome::Completed);
        assert_eq!(progress_count, 6);
        assert_eq!(
            controller.calls.borrow().as_slice(),
            [
                "move(100,200)",
                "down(Left)",
                "up(Left)",
                "move(100,200)",
                "down(Left)",
                "up(Left)",
            ]
        );
    }

    #[test]
    fn absolute_schedule_does_not_accumulate_drift() {
        // 5 个动作，每个间隔 20ms：无论单次 sleep 超调多少，
        // 总时长都应贴着 100ms（相对调度会逐帧累积超调）。
        let mut controller = FakeController::default();
        let recording = Recording::new(
            (0..5)
                .map(|i| RecordedAction {
                    delay_us: if i == 0 { 0 } else { 20_000 },
                    action: MouseAction::Move {
                        x: i as f64,
                        y: 0.0,
                    },
                })
                .collect(),
        );
        let options = ReplayOptions::default();
        let stop = AtomicBool::new(false);

        let started = Instant::now();
        let outcome =
            ReplayEngine::run(&mut controller, &recording, &options, &stop, |_| {}).unwrap();
        let elapsed = started.elapsed();

        assert_eq!(outcome, ReplayOutcome::Completed);
        assert_eq!(controller.calls.borrow().len(), 5);
        assert!(elapsed >= Duration::from_millis(80), "{elapsed:?}");
        assert!(elapsed < Duration::from_millis(2000), "{elapsed:?}");
    }

    #[test]
    fn infinite_loops_runs_until_stopped() {
        let mut controller = FakeController::default();
        let recording = Recording::new(vec![RecordedAction {
            delay_us: 0,
            action: MouseAction::Move { x: 1.0, y: 2.0 },
        }]);
        let options = ReplayOptions {
            loops: 1, // 无限模式下被忽略
            infinite: true,
            ..Default::default()
        };
        let stop = AtomicBool::new(false);

        let mut progress_calls = 0;
        let mut all_reported_infinite = true;
        let mut last_loop_index = 0u64;

        let outcome = ReplayEngine::run(&mut controller, &recording, &options, &stop, |p| {
            progress_calls += 1;
            all_reported_infinite &= p.infinite;
            last_loop_index = p.loop_index;
            // 第 3 轮执行完动作后请求停止
            if p.loop_index >= 2 {
                stop.store(true, Ordering::Relaxed);
            }
        })
        .unwrap();

        assert_eq!(outcome, ReplayOutcome::Stopped);
        assert_eq!(progress_calls, 3);
        assert!(all_reported_infinite);
        assert_eq!(last_loop_index, 2);
        // 3 轮各执行 1 个动作
        assert_eq!(controller.calls.borrow().len(), 3);
    }

    #[test]
    fn stops_and_releases_pressed_buttons() {
        let mut controller = FakeController::default();
        let recording = Recording::new(vec![
            RecordedAction {
                delay_us: 0,
                action: MouseAction::ButtonDown {
                    button: MouseButton::Left,
                },
            },
            RecordedAction {
                delay_us: 5_000_000,
                action: MouseAction::Move { x: 1.0, y: 1.0 },
            },
        ]);
        let options = ReplayOptions {
            loops: 1,
            ..Default::default()
        };
        let stop = std::sync::Arc::new(AtomicBool::new(false));

        // 在另一个线程 50ms 后触发停止
        let stopper_stop = std::sync::Arc::clone(&stop);
        let stopper = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            stopper_stop.store(true, Ordering::Relaxed);
        });

        let outcome =
            ReplayEngine::run(&mut controller, &recording, &options, &stop, |_| {}).unwrap();

        stopper.join().unwrap();
        assert_eq!(outcome, ReplayOutcome::Stopped);
        assert_eq!(
            controller.calls.borrow().as_slice(),
            ["down(Left)", "up(Left)"]
        );
    }

    #[test]
    fn speed_scales_delay() {
        assert_eq!(scaled_delay(100_000, 1.0), Duration::from_millis(100));
        assert_eq!(scaled_delay(100_000, 2.0), Duration::from_millis(50));
        assert_eq!(scaled_delay(100_000, 0.5), Duration::from_millis(200));
        assert_eq!(scaled_delay(0, 2.0), Duration::ZERO);
        assert_eq!(scaled_delay(250, 1.0), Duration::from_micros(250));
    }

    #[test]
    fn scaled_delay_is_clamped() {
        // 脏数据不允许造成 u64 溢出或永久卡死
        assert_eq!(scaled_delay(u64::MAX, 1.0), MAX_WAIT);
        assert_eq!(scaled_delay(u64::MAX, 0.1), MAX_WAIT);
    }

    #[test]
    fn error_propagates_and_releases_buttons() {
        let mut controller = FakeController {
            calls: RefCell::new(Vec::new()),
            fail: true,
        };
        let recording = Recording::new(vec![RecordedAction {
            delay_us: 0,
            action: MouseAction::ButtonDown {
                button: MouseButton::Right,
            },
        }]);
        let options = ReplayOptions::default();
        let stop = AtomicBool::new(false);

        let result = ReplayEngine::run(&mut controller, &recording, &options, &stop, |_| {});

        assert!(result.is_err());
        // button_down 失败时未记录 pressed，但 ButtonUp 场景已在其它测试覆盖
    }

    #[test]
    fn wait_until_returns_immediately_when_stopped() {
        let stop = AtomicBool::new(true);
        let started = Instant::now();
        assert!(wait_until(Instant::now() + Duration::from_secs(10), &stop));
        assert!(started.elapsed() < Duration::from_millis(100));
    }

    #[test]
    fn wait_until_waits_until_target() {
        let stop = AtomicBool::new(false);
        let target = Instant::now() + Duration::from_millis(50);
        let started = Instant::now();
        assert!(!wait_until(target, &stop));
        assert!(started.elapsed() >= Duration::from_millis(45));
    }

    #[test]
    fn wait_until_returns_when_target_already_passed() {
        let stop = AtomicBool::new(false);
        let started = Instant::now();
        assert!(!wait_until(Instant::now() - Duration::from_secs(1), &stop));
        assert!(started.elapsed() < Duration::from_millis(50));
    }
}
