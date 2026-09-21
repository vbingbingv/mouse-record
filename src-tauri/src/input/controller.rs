use crate::error::AutomationError;
use crate::model::action::{MouseAction, MouseButton};

/// 鼠标控制抽象（回放侧）。
///
/// 注意：不带 `Send` bound——macOS 上 enigo 内部的 CGEventSource
/// 不是 Send。控制器必须在使用的线程内创建和使用（回放线程即如此）。
pub trait MouseController {
    fn move_to(&mut self, x: f64, y: f64) -> Result<(), AutomationError>;

    fn button_down(&mut self, button: MouseButton) -> Result<(), AutomationError>;

    fn button_up(&mut self, button: MouseButton) -> Result<(), AutomationError>;

    fn scroll(&mut self, delta_x: i64, delta_y: i64) -> Result<(), AutomationError>;

    fn execute(&mut self, action: &MouseAction) -> Result<(), AutomationError> {
        match action {
            MouseAction::Move { x, y } => self.move_to(*x, *y),
            MouseAction::ButtonDown { button } => self.button_down(*button),
            MouseAction::ButtonUp { button } => self.button_up(*button),
            MouseAction::Wheel { delta_x, delta_y } => self.scroll(*delta_x, *delta_y),
        }
    }
}
