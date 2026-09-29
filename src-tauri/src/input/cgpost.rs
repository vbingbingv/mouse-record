//! macOS 回放后端：直接投递 CGEvent。
//!
//! 为什么不用 enigo（macOS 上）：
//! enigo 0.2.1 的 `location()` 把 `CGDisplayPixelsHigh`（像素）与
//! `NSEvent::mouseLocation().y`（点）相减，Retina 屏上算出的坐标整体偏移一个屏幕高；
//! 而它的 `button()` 正是用这个坐标给点击事件定位置，回放的点击会全部落错地方。
//! 它的滚轮还要先做“像素 → 行”的启发式换算，精度同样有损。
//!
//! 本模块的行为：
//! - 移动：绝对坐标直投；按住按键时投递对应的 Dragged 事件，拖拽轨迹与录制一致；
//! - 点击：钉在最后一次投递的位置上（不查询系统坐标，避免上面那类错位）；
//! - 滚轮：以像素为单位原样回放录制到的 point delta，与录制端精确对称。
//!
//! 鼠标控制器的创建与使用必须在同一个线程内（`CGEventSource` 不是 Send），
//! 与回放线程的生命周期一致。

use std::time::{Duration, Instant};

use core_graphics::event::{
    CGEvent, CGEventTapLocation, CGEventType, CGMouseButton, EventField, ScrollEventUnit,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;

use crate::error::AutomationError;
use crate::model::action::MouseButton;
use crate::platform;

use super::cgtap;
use super::controller::MouseController;

/// 双击判定窗口（与系统默认一致）。
const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(500);

/// 每个按键的点击计数：回放双击时 click_state 必须递增，
/// 否则系统与应用只会把它当成两次单击（例如 Finder 里双击打不开文件）。
#[derive(Debug, Default, Clone, Copy)]
struct ClickState {
    last_press: Option<Instant>,
    count: i64,
}

impl ClickState {
    fn on_press(&mut self) -> i64 {
        let now = Instant::now();
        self.count = match self.last_press {
            Some(last) if now.saturating_duration_since(last) < DOUBLE_CLICK_WINDOW => {
                self.count + 1
            }
            _ => 1,
        };
        self.last_press = Some(now);
        self.count
    }
}

/// 鼠标按键在内部状态里的下标（与 `ClickState` 数组一致）。
fn button_slot(button: MouseButton) -> Result<usize, AutomationError> {
    match button {
        MouseButton::Left => Ok(0),
        MouseButton::Right => Ok(1),
        MouseButton::Middle => Ok(2),
        // 录制端（cgtap）不会产生侧键；domain 模型保留它们是为了未来兼容
        MouseButton::Back | MouseButton::Forward => Err(AutomationError::InputBackend(format!(
            "mouse button {button:?} is not supported on macOS"
        ))),
    }
}

/// 按下 / 抬起对应的 CGEvent 类型与 CGMouseButton。
fn mouse_event_kind(
    button: MouseButton,
    down: bool,
) -> Result<(CGEventType, CGMouseButton), AutomationError> {
    button_slot(button)?;
    Ok(match (button, down) {
        (MouseButton::Left, true) => (CGEventType::LeftMouseDown, CGMouseButton::Left),
        (MouseButton::Left, false) => (CGEventType::LeftMouseUp, CGMouseButton::Left),
        (MouseButton::Right, true) => (CGEventType::RightMouseDown, CGMouseButton::Right),
        (MouseButton::Right, false) => (CGEventType::RightMouseUp, CGMouseButton::Right),
        (MouseButton::Middle, true) => (CGEventType::OtherMouseDown, CGMouseButton::Center),
        (MouseButton::Middle, false) => (CGEventType::OtherMouseUp, CGMouseButton::Center),
        _ => unreachable!("button_slot already rejected Back/Forward"),
    })
}

/// 基于 CGEvent 的鼠标控制器（macOS 回放后端）。
pub struct CgEventPostController {
    source: CGEventSource,
    /// 最后一次投递的位置：点击 / 滚轮事件必须落在光标当前位置上。
    position: Option<CGPoint>,
    /// 仍处于按下状态的按键，决定移动事件用 MouseMoved 还是 *MouseDragged。
    pressed: Vec<MouseButton>,
    clicks: [ClickState; 3],
}

impl CgEventPostController {
    pub fn new() -> Result<Self, AutomationError> {
        check_post_events_allowed()?;

        let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
            .map_err(|_| AutomationError::InputBackend("CGEventSourceCreate failed".into()))?;

        Ok(Self {
            source,
            position: None,
            pressed: Vec::new(),
            clicks: [ClickState::default(); 3],
        })
    }

    /// 当前位置：优先用输入监听线程观察到的光标位置，
    /// 其次查询系统光标（录制自启动后从未移动过鼠标、且首个动作就是点击时才会走到这里）。
    fn ensure_position(&mut self) -> Result<CGPoint, AutomationError> {
        if let Some(position) = self.position {
            return Ok(position);
        }

        let position = cgtap::last_cursor_position()
            .or_else(current_cursor_position)
            .ok_or_else(|| {
                AutomationError::InputBackend(
                    "cannot determine current cursor position for the first click".into(),
                )
            })?;

        self.position = Some(position);
        Ok(position)
    }

    /// 移动事件类型与按钮（见下方自由函数）。
    fn move_event_kind(&self) -> (CGEventType, CGMouseButton) {
        move_event_kind(&self.pressed)
    }

    fn make_mouse_event(
        &self,
        event_type: CGEventType,
        position: CGPoint,
        button: CGMouseButton,
    ) -> Result<CGEvent, AutomationError> {
        CGEvent::new_mouse_event(self.source.clone(), event_type, position, button).map_err(|_| {
            AutomationError::InputBackend(format!("create {event_type:?} event failed"))
        })
    }
}

/// 移动事件类型与按钮：按住某个键时必须投递对应的 Dragged 事件。
fn move_event_kind(pressed: &[MouseButton]) -> (CGEventType, CGMouseButton) {
    if pressed.contains(&MouseButton::Left) {
        (CGEventType::LeftMouseDragged, CGMouseButton::Left)
    } else if pressed.contains(&MouseButton::Right) {
        (CGEventType::RightMouseDragged, CGMouseButton::Right)
    } else if pressed.contains(&MouseButton::Middle) {
        (CGEventType::OtherMouseDragged, CGMouseButton::Center)
    } else {
        (CGEventType::MouseMoved, CGMouseButton::Left)
    }
}

impl MouseController for CgEventPostController {
    fn move_to(&mut self, x: f64, y: f64) -> Result<(), AutomationError> {
        let position = CGPoint::new(x, y);
        let (event_type, button) = self.move_event_kind();
        let event = self.make_mouse_event(event_type, position, button)?;

        // 真实移动事件带相对位移，部分应用（游戏、画板）会读它
        if let Some(previous) = self.position {
            event.set_integer_value_field(
                EventField::MOUSE_EVENT_DELTA_X,
                (x - previous.x).round() as i64,
            );
            event.set_integer_value_field(
                EventField::MOUSE_EVENT_DELTA_Y,
                (y - previous.y).round() as i64,
            );
        }

        event.post(CGEventTapLocation::HID);
        self.position = Some(position);
        Ok(())
    }

    fn button_down(&mut self, button: MouseButton) -> Result<(), AutomationError> {
        let position = self.ensure_position()?;
        let (event_type, cg_button) = mouse_event_kind(button, true)?;
        let click_state = self.clicks[button_slot(button)?].on_press();

        let event = self.make_mouse_event(event_type, position, cg_button)?;
        event.set_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE, click_state);
        event.post(CGEventTapLocation::HID);

        if !self.pressed.contains(&button) {
            self.pressed.push(button);
        }
        Ok(())
    }

    fn button_up(&mut self, button: MouseButton) -> Result<(), AutomationError> {
        let position = self.ensure_position()?;
        let (event_type, cg_button) = mouse_event_kind(button, false)?;
        // 抬起事件带与按下相同的 click_state
        let click_state = self.clicks[button_slot(button)?].count.max(1);

        let event = self.make_mouse_event(event_type, position, cg_button)?;
        event.set_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE, click_state);
        event.post(CGEventTapLocation::HID);

        self.pressed.retain(|pressed| *pressed != button);
        Ok(())
    }

    fn scroll(&mut self, delta_x: i64, delta_y: i64) -> Result<(), AutomationError> {
        let vertical = platform::wheel_vertical_to_output(delta_y);
        let horizontal = platform::wheel_horizontal_to_output(delta_x);

        if vertical == 0 && horizontal == 0 {
            return Ok(());
        }

        // 像素单位：录制端读的就是 point delta，原样投递即精确复现
        let event = CGEvent::new_scroll_event(
            self.source.clone(),
            ScrollEventUnit::PIXEL,
            2,
            vertical,
            horizontal,
            0,
        )
        .map_err(|_| AutomationError::InputBackend("create scroll event failed".into()))?;

        event.post(CGEventTapLocation::HID);
        Ok(())
    }
}

/// 回放需要在“辅助功能”里获得授权，否则 CGEventPost 会静默丢弃事件。
/// 这里提前拦住，把静默失败变成一条明确提示。
fn check_post_events_allowed() -> Result<(), AutomationError> {
    if platform::permissions::status().post_events {
        return Ok(());
    }
    Err(AutomationError::InputBackend(
        "accessibility permission is required to replay (System Settings → Privacy & Security → Accessibility)"
            .into(),
    ))
}

/// 查询系统光标位置（回放首个动作就是点击时的兜底）。
fn current_cursor_position() -> Option<CGPoint> {
    use std::ffi::c_void;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        /// CGEventCreate(NULL)：创建一个空事件，其位置即当前光标位置。
        fn CGEventCreate(source: *mut c_void) -> *mut c_void;
        fn CGEventGetLocation(event: *mut c_void) -> CGPoint;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: *mut c_void);
    }

    unsafe {
        let event = CGEventCreate(std::ptr::null_mut());
        if event.is_null() {
            return None;
        }
        let point = CGEventGetLocation(event);
        CFRelease(event);
        Some(point)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_counts_increment_within_double_click_window() {
        let mut state = ClickState::default();
        assert_eq!(state.on_press(), 1);
        // 立刻再按一次：双击
        assert_eq!(state.on_press(), 2);
        assert_eq!(state.on_press(), 3);
    }

    #[test]
    fn click_counts_reset_after_window() {
        let mut state = ClickState::default();
        assert_eq!(state.on_press(), 1);
        // 超时后重新从 1 开始
        state.last_press = Some(Instant::now() - DOUBLE_CLICK_WINDOW * 2);
        assert_eq!(state.on_press(), 1);
    }

    #[test]
    fn move_event_type_follows_pressed_buttons() {
        assert!(matches!(move_event_kind(&[]).0, CGEventType::MouseMoved));
        assert!(matches!(
            move_event_kind(&[MouseButton::Left]).0,
            CGEventType::LeftMouseDragged
        ));
        assert!(matches!(
            move_event_kind(&[MouseButton::Right]).0,
            CGEventType::RightMouseDragged
        ));
        assert!(matches!(
            move_event_kind(&[MouseButton::Middle]).0,
            CGEventType::OtherMouseDragged
        ));
    }

    #[test]
    fn left_button_wins_when_multiple_buttons_are_held() {
        // 录制端可能同时按下多个键，事件类型取左键，保证拖动轨迹完整
        assert!(matches!(
            move_event_kind(&[MouseButton::Left, MouseButton::Right]).0,
            CGEventType::LeftMouseDragged
        ));
    }

    #[test]
    fn side_buttons_are_rejected() {
        assert!(mouse_event_kind(MouseButton::Back, true).is_err());
        assert!(mouse_event_kind(MouseButton::Forward, false).is_err());
        assert!(button_slot(MouseButton::Back).is_err());
    }

    #[test]
    fn event_kinds_match_button_and_direction() {
        let (event_type, button) = mouse_event_kind(MouseButton::Left, true).unwrap();
        assert!(matches!(event_type, CGEventType::LeftMouseDown));
        assert!(matches!(button, CGMouseButton::Left));

        let (event_type, button) = mouse_event_kind(MouseButton::Left, false).unwrap();
        assert!(matches!(event_type, CGEventType::LeftMouseUp));
        assert!(matches!(button, CGMouseButton::Left));

        let (event_type, button) = mouse_event_kind(MouseButton::Middle, true).unwrap();
        assert!(matches!(event_type, CGEventType::OtherMouseDown));
        assert!(matches!(button, CGMouseButton::Center));

        let (event_type, button) = mouse_event_kind(MouseButton::Right, false).unwrap();
        assert!(matches!(event_type, CGEventType::RightMouseUp));
        assert!(matches!(button, CGMouseButton::Right));
    }

    #[test]
    fn button_slots_are_stable() {
        assert_eq!(button_slot(MouseButton::Left).unwrap(), 0);
        assert_eq!(button_slot(MouseButton::Right).unwrap(), 1);
        assert_eq!(button_slot(MouseButton::Middle).unwrap(), 2);
    }
}
