//! macOS 专用全局输入事件源：直接创建 CGEventTap。
//!
//! 为什么不用 rdev（macOS）：
//! rdev 0.5.3 的 convert() 只映射 MouseMoved，而 macOS 上按住鼠标按钮
//! 移动时系统发送的是 LeftMouseDragged / RightMouseDragged /
//! OtherMouseDragged——这些事件全部落到 `_ => None` 被丢弃，
//! 导致拖拽过程完全不被录制；OtherMouse（中键）同样被丢弃。
//!
//! 本模块的 tap 订阅完整事件集，拖拽事件映射为普通 Move。
//!
//! 键盘事件同在这个 tap 里处理：Ctrl+8 / Ctrl+9 是与 Windows 对齐的全局功能热键，
//! 而 listen-only tap 无法拦截事件，热键按键会照常传给前台应用（与 Windows 一致）。
#![allow(improper_ctypes_definitions)]

use std::ffi::c_void;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use core_graphics::event::{CGEvent, CGEventFlags, EventField};
use core_graphics::geometry::CGPoint;

use crate::error::AutomationError;
use crate::model::action::{MouseAction, MouseButton};

use super::source::{
    EventSink, Hotkey, HotkeyFilter, MouseEventSource, RawInputEvent, RawMouseEvent,
};

// CGEventType 原始类型码
// https://developer.apple.com/documentation/coregraphics/cgeventtype
const LEFT_MOUSE_DOWN: u32 = 1;
const LEFT_MOUSE_UP: u32 = 2;
const RIGHT_MOUSE_DOWN: u32 = 3;
const RIGHT_MOUSE_UP: u32 = 4;
const MOUSE_MOVED: u32 = 5;
const LEFT_MOUSE_DRAGGED: u32 = 6;
const RIGHT_MOUSE_DRAGGED: u32 = 7;
const KEY_DOWN: u32 = 10;
const KEY_UP: u32 = 11;
const SCROLL_WHEEL: u32 = 22;
const OTHER_MOUSE_DOWN: u32 = 25;
const OTHER_MOUSE_UP: u32 = 26;
const OTHER_MOUSE_DRAGGED: u32 = 30;
const TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
const TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;

/// MOUSE_EVENT_BUTTON_NUMBER 字段里表示中键的值
const BUTTON_NUMBER_MIDDLE: i64 = 2;

/// 热键基础键的键码（kVK_ANSI_8 / kVK_ANSI_9）。
///
/// 用物理键位而非字符，键盘布局不同也能对齐；小键盘 8/9（kVK_Keypad8/9 = 91/92）
/// 与 Windows 侧一致地不参与热键。
const KEYCODE_ANSI_8: i64 = 28;
const KEYCODE_ANSI_9: i64 = 25;

const KCG_HID_EVENT_TAP: u32 = 0;
const KCG_HEAD_INSERT_EVENT_TAP: u32 = 0;
const KCG_EVENT_TAP_OPTION_LISTEN_ONLY: u32 = 1;

/// 我们关心的全部事件位掩码。
///
/// 键盘事件（KeyDown/KeyUp）同样订阅：热键判定需要它们。
/// 订阅键盘事件后 tap 需要“输入监控”权限（鼠标事件只需要辅助功能），
/// 授权引导见 `crate::platform::permissions`。
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
        KEY_DOWN,
        KEY_UP,
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

/// 热键基础键的键码 → 热键。
fn hotkey_for_keycode(keycode: i64) -> Option<Hotkey> {
    match keycode {
        KEYCODE_ANSI_8 => Some(Hotkey::ToggleRecording),
        KEYCODE_ANSI_9 => Some(Hotkey::ToggleReplay),
        _ => None,
    }
}

/// 键盘事件分类结果（纯逻辑，供 HotkeyFilter 消费）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyEvent {
    /// 基础键按下，附带该事件上的 Ctrl 状态。
    /// macOS 的键事件自带修饰键位，不必像 Windows 那样单独跟踪 Ctrl。
    Pressed { hotkey: Hotkey, ctrl: bool },
    /// 基础键松开（只复位去抖状态）。
    Released(Hotkey),
}

/// 纯映射逻辑：事件类型码 + 键码 + 修饰键位 + auto-repeat → 键盘事件种类。
///
/// auto-repeat（按住不放的连续 KeyDown）直接丢弃：否则会反复触发开关，
/// 只能等 KeyUp 之后重新按下才算一次。
fn classify_key(
    event_type: u32,
    keycode: i64,
    flags: CGEventFlags,
    autorepeat: bool,
) -> Option<KeyEvent> {
    let hotkey = hotkey_for_keycode(keycode)?;
    match event_type {
        KEY_DOWN if autorepeat => None,
        KEY_DOWN => Some(KeyEvent::Pressed {
            hotkey,
            ctrl: flags.contains(CGEventFlags::CGEventFlagControl),
        }),
        KEY_UP => Some(KeyEvent::Released(hotkey)),
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

/// tap 运行期状态：事件出口 + 热键状态机（`listen_forever` 启动时写入）。
///
/// tap 回调必须是 C 函数指针（无法捕获闭包状态），所以状态放在静态里；
/// 热键状态机与 Windows 用同一份 `HotkeyFilter`，语义完全一致。
struct TapRuntime {
    sink: EventSink,
    hotkeys: HotkeyFilter,
    /// 最近一次在系统事件流里看到的鼠标位置。
    /// 回放侧在“录制的第一个动作就是点击”（没有前置 Move）时用它定位。
    last_cursor: Option<CGPoint>,
}

static RUNTIME: Mutex<Option<TapRuntime>> = Mutex::new(None);
/// 当前 tap 的 mach port（超时被系统禁用时用于重新启用）。
static TAP_PORT: AtomicPtr<c_void> = AtomicPtr::new(null_mut());

/// 取出运行期状态。锁中毒（回调里绝不会 panic，但保守处理）时恢复数据继续使用。
fn with_runtime<T>(f: impl FnOnce(&mut Option<TapRuntime>) -> T) -> T {
    let mut guard = RUNTIME
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    f(&mut guard)
}

/// 最近一次观察到的鼠标位置（回放定位用）。tap 未运行或还没收到鼠标事件时为 None。
pub fn last_cursor_position() -> Option<CGPoint> {
    with_runtime(|runtime| runtime.as_ref().and_then(|runtime| runtime.last_cursor))
}

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

    let is_key_event = event_type == KEY_DOWN || event_type == KEY_UP;

    // 键盘事件只做热键判定；鼠标事件才需要按钮号
    let mouse_kind = if is_key_event {
        None
    } else {
        classify_event(
            event_type,
            event.get_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER),
        )
    };

    let key_event = if is_key_event {
        classify_key(
            event_type,
            event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE),
            event.get_flags(),
            event.get_integer_value_field(EventField::KEYBOARD_EVENT_AUTOREPEAT) != 0,
        )
    } else {
        None
    };

    if mouse_kind.is_none() && key_event.is_none() {
        return event;
    }

    let now = Instant::now();
    // 回调内只做非阻塞投递：持锁时间必须尽可能短
    with_runtime(|runtime| {
        let Some(runtime) = runtime.as_mut() else {
            return;
        };

        if let Some(kind) = mouse_kind {
            // 所有鼠标事件都带当前光标位置，顺手记下供回放定位
            let cursor = event.location();
            runtime.last_cursor = Some(cursor);

            let raw = match kind {
                EventKind::Move => RawInputEvent::Mouse(RawMouseEvent {
                    time: now,
                    action: MouseAction::Move {
                        x: cursor.x,
                        y: cursor.y,
                    },
                }),
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
                        delta_y: event.get_integer_value_field(
                            EventField::SCROLL_WHEEL_EVENT_POINT_DELTA_AXIS_1,
                        ),
                        delta_x: event.get_integer_value_field(
                            EventField::SCROLL_WHEEL_EVENT_POINT_DELTA_AXIS_2,
                        ),
                    },
                }),
            };
            (runtime.sink)(raw);
            return;
        }

        // 与 Windows 侧同样的语义：Ctrl 状态先更新，再按“按下沿”判定
        if let Some(key) = key_event {
            match key {
                KeyEvent::Pressed { hotkey, ctrl } => {
                    runtime.hotkeys.on_ctrl(ctrl);
                    if let Some(hotkey) = runtime.hotkeys.on_press(hotkey) {
                        (runtime.sink)(RawInputEvent::Hotkey(hotkey));
                    }
                }
                KeyEvent::Released(hotkey) => runtime.hotkeys.on_release(hotkey),
            }
        }
    });

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
            // 每次重建 tap 都从干净的热键状态开始（避免上一次残留的“已按下”）
            with_runtime(|slot| {
                *slot = Some(TapRuntime {
                    sink,
                    hotkeys: HotkeyFilter::default(),
                    last_cursor: None,
                });
            });

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
                    "CGEventTapCreate failed (check input monitoring / accessibility permission)"
                        .into(),
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
        assert_eq!(
            classify_event(OTHER_MOUSE_DRAGGED, 2),
            Some(EventKind::Move)
        );
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

    #[test]
    fn mask_includes_key_events_for_hotkeys() {
        let mask = event_mask();
        assert!(mask & (1 << KEY_DOWN) != 0, "mask missing key down");
        assert!(mask & (1 << KEY_UP) != 0, "mask missing key up");
    }

    #[test]
    fn key_8_and_9_map_to_hotkeys() {
        assert_eq!(
            classify_key(
                KEY_DOWN,
                KEYCODE_ANSI_8,
                CGEventFlags::CGEventFlagControl,
                false
            ),
            Some(KeyEvent::Pressed {
                hotkey: Hotkey::ToggleRecording,
                ctrl: true,
            })
        );
        assert_eq!(
            classify_key(
                KEY_DOWN,
                KEYCODE_ANSI_9,
                CGEventFlags::CGEventFlagControl,
                false
            ),
            Some(KeyEvent::Pressed {
                hotkey: Hotkey::ToggleReplay,
                ctrl: true,
            })
        );
        assert_eq!(
            classify_key(KEY_UP, KEYCODE_ANSI_8, CGEventFlags::CGEventFlagNull, false),
            Some(KeyEvent::Released(Hotkey::ToggleRecording))
        );
        assert_eq!(
            classify_key(KEY_UP, KEYCODE_ANSI_9, CGEventFlags::CGEventFlagNull, false),
            Some(KeyEvent::Released(Hotkey::ToggleReplay))
        );
    }

    #[test]
    fn key_events_carry_ctrl_state() {
        // 没有 Ctrl 时同样上报按下，交由 HotkeyFilter 拒绝（“先按数字键再补 Ctrl 不触发”）
        assert_eq!(
            classify_key(
                KEY_DOWN,
                KEYCODE_ANSI_8,
                CGEventFlags::CGEventFlagNull,
                false
            ),
            Some(KeyEvent::Pressed {
                hotkey: Hotkey::ToggleRecording,
                ctrl: false,
            })
        );
        // 其它修饰键不算 Ctrl
        assert_eq!(
            classify_key(
                KEY_DOWN,
                KEYCODE_ANSI_9,
                CGEventFlags::CGEventFlagShift,
                false
            ),
            Some(KeyEvent::Pressed {
                hotkey: Hotkey::ToggleReplay,
                ctrl: false,
            })
        );
    }

    #[test]
    fn key_autorepeat_is_dropped() {
        // 按住不放：重复的 KeyDown 不能反复开关
        assert_eq!(
            classify_key(
                KEY_DOWN,
                KEYCODE_ANSI_8,
                CGEventFlags::CGEventFlagControl,
                true
            ),
            None
        );
        // 松开仍然要处理，否则去抖状态会一直停在“已按下”
        assert_eq!(
            classify_key(KEY_UP, KEYCODE_ANSI_8, CGEventFlags::CGEventFlagNull, true),
            Some(KeyEvent::Released(Hotkey::ToggleRecording))
        );
    }

    #[test]
    fn non_hotkey_keys_are_ignored() {
        // 小键盘 8/9（kVK_Keypad8/9）与 Windows 侧一致地不参与热键
        assert_eq!(
            classify_key(KEY_DOWN, 91, CGEventFlags::CGEventFlagControl, false),
            None
        );
        assert_eq!(
            classify_key(KEY_DOWN, 92, CGEventFlags::CGEventFlagControl, false),
            None
        );
        // 主键盘区其它数字键
        assert_eq!(
            classify_key(KEY_DOWN, 26, CGEventFlags::CGEventFlagControl, false),
            None
        );
        // 非键盘事件类型
        assert_eq!(
            classify_key(
                MOUSE_MOVED,
                KEYCODE_ANSI_8,
                CGEventFlags::CGEventFlagNull,
                false
            ),
            None
        );
    }
}
