use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use rdev::{Event, EventType, Key};

use crate::error::AutomationError;
use crate::model::action::{MouseAction, MouseButton};

use super::source::{
    EventSink, Hotkey, HotkeyFilter, MouseEventSource, RawInputEvent, RawMouseEvent,
};

/// 通用虚拟键码：极少数情况下（注入事件、特殊键盘驱动）钩子给出的
/// 不是左右专用码而是 VK_CONTROL，这里用于兼容识别 Ctrl。
const VK_CONTROL: u32 = 0x11;

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

        // 热键状态（Ctrl + 按下沿去抖）：rdev::listen 的回调是 FnMut，就地保存在闭包里
        let mut hotkeys = HotkeyFilter::default();

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
                Some(Classified::HotkeyPressed(hotkey)) => {
                    // 按住时的 auto-repeat 只发 KeyPress；组合键必须 Ctrl 按住
                    match hotkeys.on_press(hotkey) {
                        Some(hotkey) => RawInputEvent::Hotkey(hotkey),
                        None => return,
                    }
                }
                Some(Classified::HotkeyReleased(hotkey)) => {
                    hotkeys.on_release(hotkey);
                    return;
                }
                Some(Classified::CtrlChanged(down)) => {
                    hotkeys.on_ctrl(down);
                    return;
                }
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
    /// Ctrl 按下/抬起（只更新状态，不产生业务事件）。
    CtrlChanged(bool),
    HotkeyPressed(Hotkey),
    HotkeyReleased(Hotkey),
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

        // Ctrl+8 / Ctrl+9 是全局功能热键；数字键的释放事件
        // 只用于维护去抖状态，不产生业务动作
        EventType::KeyPress(Key::Num8) => Some(Classified::HotkeyPressed(Hotkey::ToggleRecording)),
        EventType::KeyPress(Key::Num9) => Some(Classified::HotkeyPressed(Hotkey::ToggleReplay)),
        EventType::KeyRelease(Key::Num8) => {
            Some(Classified::HotkeyReleased(Hotkey::ToggleRecording))
        }
        EventType::KeyRelease(Key::Num9) => {
            Some(Classified::HotkeyReleased(Hotkey::ToggleReplay))
        }

        // Ctrl：rdev 只给事件流，组合判断所需的“当前是否按住”自己维护
        EventType::KeyPress(Key::ControlLeft | Key::ControlRight | Key::Unknown(VK_CONTROL)) => {
            Some(Classified::CtrlChanged(true))
        }
        EventType::KeyRelease(Key::ControlLeft | Key::ControlRight | Key::Unknown(VK_CONTROL)) => {
            Some(Classified::CtrlChanged(false))
        }

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
    fn escape_key_is_ignored() {
        // ESC 不再作为紧急停止入口，与其它普通按键一样被忽略
        assert!(matches!(classify(&EventType::KeyPress(Key::Escape)), None));
        assert!(matches!(classify(&EventType::KeyRelease(Key::Escape)), None));
        assert!(matches!(classify(&EventType::KeyPress(Key::KeyA)), None));
    }

    #[test]
    fn classifies_hotkey_press_and_release() {
        assert!(matches!(
            classify(&EventType::KeyPress(Key::Num8)),
            Some(Classified::HotkeyPressed(Hotkey::ToggleRecording))
        ));
        assert!(matches!(
            classify(&EventType::KeyPress(Key::Num9)),
            Some(Classified::HotkeyPressed(Hotkey::ToggleReplay))
        ));
        assert!(matches!(
            classify(&EventType::KeyRelease(Key::Num8)),
            Some(Classified::HotkeyReleased(Hotkey::ToggleRecording))
        ));
        assert!(matches!(
            classify(&EventType::KeyRelease(Key::Num9)),
            Some(Classified::HotkeyReleased(Hotkey::ToggleReplay))
        ));
        // 相邻数字键不参与
        assert!(matches!(classify(&EventType::KeyPress(Key::Num7)), None));
        // 旧的 F8/F9 不再是热键
        assert!(matches!(classify(&EventType::KeyPress(Key::F8)), None));
        assert!(matches!(classify(&EventType::KeyPress(Key::F9)), None));
    }

    #[test]
    fn classifies_ctrl_press_and_release() {
        assert!(matches!(
            classify(&EventType::KeyPress(Key::ControlLeft)),
            Some(Classified::CtrlChanged(true))
        ));
        assert!(matches!(
            classify(&EventType::KeyRelease(Key::ControlRight)),
            Some(Classified::CtrlChanged(false))
        ));
        assert!(matches!(
            classify(&EventType::KeyPress(Key::Unknown(VK_CONTROL))),
            Some(Classified::CtrlChanged(true))
        ));
        // Alt 不再参与热键：与普通按键一样被忽略
        assert!(matches!(classify(&EventType::KeyPress(Key::Alt)), None));
        assert!(matches!(classify(&EventType::KeyPress(Key::AltGr)), None));
    }

    #[test]
    fn ignores_unmapped_buttons() {
        assert!(matches!(
            classify(&EventType::ButtonPress(rdev::Button::Unknown(8))),
            None
        ));
    }
}
