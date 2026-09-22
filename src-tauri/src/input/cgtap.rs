//! macOS 专用全局输入事件源：直接创建 CGEventTap。
//!
//! 为什么不用 rdev（macOS）：
//! rdev 0.5.3 的 convert() 只映射 MouseMoved，而 macOS 上按住鼠标按钮
//! 移动时系统发送的是 LeftMouseDragged / RightMouseDragged /
//! OtherMouseDragged——这些事件全部落到 `_ => None` 被丢弃，
//! 导致拖拽过程完全不被录制；OtherMouse（中键）同样被丢弃。
//!
//! 本模块的 tap 订阅完整事件集，拖拽事件映射为普通 Move。
#![allow(improper_ctypes_definitions)]

use std::ffi::c_void;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use core_graphics::event::{CGEvent, EventField};
use core_graphics::geometry::CGPoint;

use crate::error::AutomationError;
use crate::model::action::{MouseAction, MouseButton};

use super::source::{EventSink, MouseEventSource, RawInputEvent, RawMouseEvent};

// CGEventType 原始类型码
// https://developer.apple.com/documentation/coregraphics/cgeventtype
const LEFT_MOUSE_DOWN: u32 = 1;
const LEFT_MOUSE_UP: u32 = 2;
const RIGHT_MOUSE_DOWN: u32 = 3;
const RIGHT_MOUSE_UP: u32 = 4;
const MOUSE_MOVED: u32 = 5;
const LEFT_MOUSE_DRAGGED: u32 = 6;
const RIGHT_MOUSE_DRAGGED: u32 = 7;
const SCROLL_WHEEL: u32 = 22;
const OTHER_MOUSE_DOWN: u32 = 25;
const OTHER_MOUSE_UP: u32 = 26;
const OTHER_MOUSE_DRAGGED: u32 = 30;
const TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
const TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;

/// MOUSE_EVENT_BUTTON_NUMBER 字段里表示中键的值
const BUTTON_NUMBER_MIDDLE: i64 = 2;

const KCG_HID_EVENT_TAP: u32 = 0;
const KCG_HEAD_INSERT_EVENT_TAP: u32 = 0;
const KCG_EVENT_TAP_OPTION_LISTEN_ONLY: u32 = 1;

/// 我们关心的全部事件位掩码。
fn event_mask() -> u64 {
    [
        LEFT_MOUSE_DOWN,
        LEFT_MOUSE_UP,
        RIGHT_MOUSE_DOWN,
        RIGHT_MOUSE_UP,
        MOUSE_MOVED,
        LEFT_MOUSE_DRAGGED,
        RIGHT_MOUSE_DRAGGED,
        OTHER_MOUSE_DOWN,
        OTHER_MOUSE_UP,
        OTHER_MOUSE_DRAGGED,
        SCROLL_WHEEL,
    ]
    .into_iter()
    .fold(0u64, |mask, t| mask | (1 << t))
}

/// 事件分类结果（不含坐标/滚轮增量，由回调从 CGEvent 上读取）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EventKind {
    Move,
    ButtonDown(MouseButton),
    ButtonUp(MouseButton),
    Wheel,
}

/// 纯映射逻辑：事件类型码 + 按钮号 → 事件种类。
fn classify_event(event_type: u32, button_number: i64) -> Option<EventKind> {
    match event_type {
        // 关键：拖拽事件（按住按钮移动）与普通移动统一映射为 Move
        MOUSE_MOVED | LEFT_MOUSE_DRAGGED | RIGHT_MOUSE_DRAGGED | OTHER_MOUSE_DRAGGED => {
            Some(EventKind::Move)
        }

        LEFT_MOUSE_DOWN => Some(EventKind::ButtonDown(MouseButton::Left)),
        LEFT_MOUSE_UP => Some(EventKind::ButtonUp(MouseButton::Left)),
        RIGHT_MOUSE_DOWN => Some(EventKind::ButtonDown(MouseButton::Right)),
        RIGHT_MOUSE_UP => Some(EventKind::ButtonUp(MouseButton::Right)),

        // OtherMouse 涵盖中键与侧键，用按钮号区分；
        // 侧键（3/4）当前回放后端不支持，忽略。
        OTHER_MOUSE_DOWN => match button_number {
            BUTTON_NUMBER_MIDDLE => Some(EventKind::ButtonDown(MouseButton::Middle)),
            _ => None,
        },
        OTHER_MOUSE_UP => match button_number {
            BUTTON_NUMBER_MIDDLE => Some(EventKind::ButtonUp(MouseButton::Middle)),
            _ => None,
        },

        SCROLL_WHEEL => Some(EventKind::Wheel),

        _ => None,
    }
}

type CFMachPortRef = *mut c_void;

// 以下 FFI 签名与 CoreGraphics / CoreFoundation 头文件一致，
// 事件回调按值传 CGEvent（内部为单指针，ABI 等价，rdev 同样用法）。
type CGEventTapCallback = unsafe extern "C" fn(
    proxy: *mut c_void,
    event_type: u32,
    event: CGEvent,
    user_info: *mut c_void,
) -> CGEvent;

#[link(name = "CoreGraphics", kind = "framework")]
#[allow(improper_ctypes)]
extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: CGEventTapCallback,
        user_info: *mut c_void,
    ) -> CFMachPortRef;

    fn CGEventTapEnable(tap: CFMachPortRef, enable: u32);
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFMachPortCreateRunLoopSource(
        allocator: *mut c_void,
        port: CFMachPortRef,
        order: u64,
    ) -> *mut c_void;

    fn CFRunLoopAddSource(run_loop: *mut c_void, source: *mut c_void, mode: *const c_void);

    fn CFRunLoopGetCurrent() -> *mut c_void;
    fn CFRunLoopRun();

    static kCFRunLoopCommonModes: *const c_void;
}

/// tap 回调分发的 sink（listen_forever 启动时写入）。
static SINK: Mutex<Option<EventSink>> = Mutex::new(None);
/// 当前 tap 的 mach port（超时被系统禁用时用于重新启用）。
static TAP_PORT: AtomicPtr<c_void> = AtomicPtr::new(null_mut());

unsafe extern "C" fn raw_callback(
    _proxy: *mut c_void,
    event_type: u32,
    event: CGEvent,
    _user_info: *mut c_void,
) -> CGEvent {
    // tap 因超时/用户切换被系统禁用：立即重新启用，避免静默丢失后续事件
    if event_type == TAP_DISABLED_BY_TIMEOUT || event_type == TAP_DISABLED_BY_USER_INPUT {
        let port = TAP_PORT.load(Ordering::Acquire);
        if !port.is_null() {
            CGEventTapEnable(port, 1);
        }
        return event;
    }

    let button_number = event.get_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER);

    let Some(kind) = classify_event(event_type, button_number) else {
        return event;
    };

    let now = Instant::now();
    let raw = match kind {
        EventKind::Move => {
            let CGPoint { x, y } = event.location();
            RawInputEvent::Mouse(RawMouseEvent {
                time: now,
                action: MouseAction::Move { x, y },
            })
        }
        EventKind::ButtonDown(button) => RawInputEvent::Mouse(RawMouseEvent {
            time: now,
            action: MouseAction::ButtonDown { button },
        }),
        EventKind::ButtonUp(button) => RawInputEvent::Mouse(RawMouseEvent {
            time: now,
            action: MouseAction::ButtonUp { button },
        }),
        EventKind::Wheel => RawInputEvent::Mouse(RawMouseEvent {
            time: now,
            action: MouseAction::Wheel {
                delta_y: event
                    .get_integer_value_field(EventField::SCROLL_WHEEL_EVENT_POINT_DELTA_AXIS_1),
                delta_x: event
                    .get_integer_value_field(EventField::SCROLL_WHEEL_EVENT_POINT_DELTA_AXIS_2),
            },
        }),
    };

    if let Ok(sink_slot) = SINK.lock() {
        if let Some(sink) = sink_slot.as_ref() {
            sink(raw);
        }
    }

    event
}

/// 基于 CGEventTap 的全局输入事件源（macOS）。
pub struct CgEventTapSource {
    running: Arc<AtomicBool>,
}

impl CgEventTapSource {
    pub fn new(running: Arc<AtomicBool>) -> Self {
        Self { running }
    }
}

impl MouseEventSource for CgEventTapSource {
    /// 阻塞当前线程直到 runloop 退出（正常情况下永不返回）。
    /// 未获得辅助功能权限时 CGEventTapCreate 返回空指针 → 返回 Err。
    fn listen_forever(&self, sink: EventSink) -> Result<(), AutomationError> {
        unsafe {
            if let Ok(mut slot) = SINK.lock() {
                *slot = Some(sink);
            }

            let tap = CGEventTapCreate(
                KCG_HID_EVENT_TAP,
                KCG_HEAD_INSERT_EVENT_TAP,
                KCG_EVENT_TAP_OPTION_LISTEN_ONLY,
                event_mask(),
                raw_callback,
                null_mut(),
            );
            if tap.is_null() {
                self.running.store(false, Ordering::Release);
                return Err(AutomationError::ListenerUnavailable(
                    "CGEventTapCreate failed (check accessibility permission)".into(),
                ));
            }
            TAP_PORT.store(tap, Ordering::Release);

            let source = CFMachPortCreateRunLoopSource(null_mut(), tap, 0);
            if source.is_null() {
                self.running.store(false, Ordering::Release);
                return Err(AutomationError::ListenerUnavailable(
                    "CFMachPortCreateRunLoopSource failed".into(),
                ));
            }

            let run_loop = CFRunLoopGetCurrent();
            CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);

            CGEventTapEnable(tap, 1);
            self.running.store(true, Ordering::Release);
            CFRunLoopRun();
        }

        self.running.store(false, Ordering::Release);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dragged_events_map_to_move() {
        // 这正是 rdev 丢事件的根源：三种拖拽必须全部映射为 Move
        assert_eq!(classify_event(MOUSE_MOVED, 0), Some(EventKind::Move));
        assert_eq!(classify_event(LEFT_MOUSE_DRAGGED, 0), Some(EventKind::Move));
        assert_eq!(
            classify_event(RIGHT_MOUSE_DRAGGED, 0),
            Some(EventKind::Move)
        );
        assert_eq!(classify_event(OTHER_MOUSE_DRAGGED, 2), Some(EventKind::Move));
    }

    #[test]
    fn other_mouse_maps_middle_and_ignores_side_buttons() {
        assert_eq!(
            classify_event(OTHER_MOUSE_DOWN, 2),
            Some(EventKind::ButtonDown(MouseButton::Middle))
        );
        assert_eq!(
            classify_event(OTHER_MOUSE_UP, 2),
            Some(EventKind::ButtonUp(MouseButton::Middle))
        );
        // 侧键（3/4）当前不支持
        assert_eq!(classify_event(OTHER_MOUSE_DOWN, 3), None);
        assert_eq!(classify_event(OTHER_MOUSE_UP, 4), None);
    }

    #[test]
    fn wheel_and_basic_buttons() {
        assert_eq!(classify_event(SCROLL_WHEEL, 0), Some(EventKind::Wheel));
        assert_eq!(
            classify_event(LEFT_MOUSE_DOWN, 0),
            Some(EventKind::ButtonDown(MouseButton::Left))
        );
        assert_eq!(
            classify_event(RIGHT_MOUSE_UP, 0),
            Some(EventKind::ButtonUp(MouseButton::Right))
        );
    }

    #[test]
    fn mask_includes_drag_and_other_mouse() {
        let mask = event_mask();
        for t in [
            MOUSE_MOVED,
            LEFT_MOUSE_DRAGGED,
            RIGHT_MOUSE_DRAGGED,
            OTHER_MOUSE_DRAGGED,
            OTHER_MOUSE_DOWN,
            OTHER_MOUSE_UP,
            SCROLL_WHEEL,
        ] {
            assert!(mask & (1 << t) != 0, "mask missing event type {t}");
        }
    }
}
