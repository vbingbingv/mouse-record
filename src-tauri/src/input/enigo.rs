use enigo::{Axis, Button, Coordinate, Direction, Enigo, Mouse, Settings};

use crate::error::AutomationError;
use crate::model::action::MouseButton;
use crate::platform;

use super::controller::MouseController;

/// 基于 enigo 的鼠标控制器。
pub struct EnigoMouseController {
    enigo: Enigo,
}

impl EnigoMouseController {
    pub fn new() -> Result<Self, AutomationError> {
        let enigo = Enigo::new(&Settings::default())
            .map_err(|e| AutomationError::InputBackend(format!("{e:?}")))?;
        Ok(Self { enigo })
    }
}

impl Default for EnigoMouseController {
    fn default() -> Self {
        Self::new().expect("failed to create enigo controller")
    }
}

impl MouseController for EnigoMouseController {
    fn move_to(&mut self, x: f64, y: f64) -> Result<(), AutomationError> {
        self.enigo
            .move_mouse(x.round() as i32, y.round() as i32, Coordinate::Abs)
            .map_err(|e| AutomationError::InputBackend(format!("{e:?}")))
    }

    fn button_down(&mut self, button: MouseButton) -> Result<(), AutomationError> {
        self.enigo
            .button(to_enigo_button(button)?, Direction::Press)
            .map_err(|e| AutomationError::InputBackend(format!("{e:?}")))
    }

    fn button_up(&mut self, button: MouseButton) -> Result<(), AutomationError> {
        self.enigo
            .button(to_enigo_button(button)?, Direction::Release)
            .map_err(|e| AutomationError::InputBackend(format!("{e:?}")))
    }

    fn scroll(&mut self, delta_x: i64, delta_y: i64) -> Result<(), AutomationError> {
        let vertical = platform::wheel_vertical_to_enigo(delta_y);
        let horizontal = platform::wheel_horizontal_to_enigo(delta_x);

        if vertical != 0 {
            self.enigo
                .scroll(vertical, Axis::Vertical)
                .map_err(|e| AutomationError::InputBackend(format!("{e:?}")))?;
        }

        if horizontal != 0 {
            self.enigo
                .scroll(horizontal, Axis::Horizontal)
                .map_err(|e| AutomationError::InputBackend(format!("{e:?}")))?;
        }

        Ok(())
    }
}

fn to_enigo_button(button: MouseButton) -> Result<Button, AutomationError> {
    match button {
        MouseButton::Left => Ok(Button::Left),
        MouseButton::Middle => Ok(Button::Middle),
        MouseButton::Right => Ok(Button::Right),
        // enigo 0.2 未提供 Back/Forward；录制源（rdev）也不会产生这两种按钮，
        // 领域模型保留它们是为了未来兼容。
        MouseButton::Back | MouseButton::Forward => Err(AutomationError::InputBackend(format!(
            "mouse button {button:?} is not supported by the current enigo backend"
        ))),
    }
}
