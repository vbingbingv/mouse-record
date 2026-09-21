use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use rdev::{Event, EventType, Key};

use crate::error::AutomationError;
use crate::model::action::{MouseAction, MouseButton};

use super::source::{EventSink, MouseEventSource, RawInputEvent, RawMouseEvent};

/// 基于 rdev 的全局输入事件源。
pub struct RdevEventSource {
    running: Arc<AtomicBool>,
}

impl RdevEventSource {
    pub fn new(running: Arc<AtomicBool>) -> Self {
        Self { running }
    }
}

impl MouseEventSource for RdevEventSource {
    fn listen_forever(&self, sink: EventSink) -> Result<(), AutomationError> {
        self.running.store(true, Ordering::Release);

        let result = rdev::listen(move |event: Event| {
            let raw = match classify(&event.event_type) {
                Some(Classified::Mouse(action)) => {
                    // 在回调线程内打点：rdev 自身的 event.time 也是回调时刻
                    // 打的（非 CGEvent 时间戳），这里等价且使用单调时钟。
                    RawInputEvent::Mouse(RawMouseEvent {
                        time: Instant::now(),
                        action,
                    })
                }
                Some(Classified::EscapePressed) => RawInputEvent::EscapePressed,
                None => return,
            };
            sink(raw);
        });

        self.running.store(false, Ordering::Release);

        result.map_err(|e| AutomationError::ListenerUnavailable(format!("{e:?}")))
    }
}

/// rdev 事件到业务动作的映射结果（不含时间戳）。
enum Classified {
    Mouse(MouseAction),
    EscapePressed,
}

fn classify(event_type: &EventType) -> Option<Classified> {
    match event_type {
        EventType::MouseMove { x, y } => {
            Some(Classified::Mouse(MouseAction::Move { x: *x, y: *y }))
        }

        EventType::ButtonPress(button) => Some(Classified::Mouse(MouseAction::ButtonDown {
            button: from_rdev_button(*button)?,
        })),

        EventType::ButtonRelease(button) => Some(Classified::Mouse(MouseAction::ButtonUp {
            button: from_rdev_button(*button)?,
        })),

        EventType::Wheel { delta_x, delta_y } => Some(Classified::Mouse(MouseAction::Wheel {
            delta_x: *delta_x,
            delta_y: *delta_y,
        })),

        EventType::KeyPress(Key::Escape) => Some(Classified::EscapePressed),

        _ => None,
    }
}

fn from_rdev_button(button: rdev::Button) -> Option<MouseButton> {
    match button {
        rdev::Button::Left => Some(MouseButton::Left),
        rdev::Button::Middle => Some(MouseButton::Middle),
        rdev::Button::Right => Some(MouseButton::Right),
        // rdev 0.5 没有 Back/Forward 变体；滚轮按钮等在此不作为鼠标按钮处理
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_mouse_events() {
        assert!(matches!(
            classify(&EventType::MouseMove { x: 10.0, y: -20.0 }),
            Some(Classified::Mouse(MouseAction::Move { x: 10.0, y: -20.0 }))
        ));

        assert!(matches!(
            classify(&EventType::ButtonPress(rdev::Button::Right)),
            Some(Classified::Mouse(MouseAction::ButtonDown {
                button: MouseButton::Right
            }))
        ));

        assert!(matches!(
            classify(&EventType::Wheel {
                delta_x: 0,
                delta_y: 3
            }),
            Some(Classified::Mouse(MouseAction::Wheel {
                delta_x: 0,
                delta_y: 3
            }))
        ));
    }

    #[test]
    fn classifies_escape_press_only() {
        assert!(matches!(
            classify(&EventType::KeyPress(Key::Escape)),
            Some(Classified::EscapePressed)
        ));
        assert!(matches!(
            classify(&EventType::KeyRelease(Key::Escape)),
            None
        ));
        assert!(matches!(classify(&EventType::KeyPress(Key::KeyA)), None));
    }

    #[test]
    fn ignores_unmapped_buttons() {
        assert!(matches!(
            classify(&EventType::ButtonPress(rdev::Button::Unknown(8))),
            None
        ));
    }
}
