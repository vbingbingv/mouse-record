use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    Back,
    Forward,
}

impl MouseButton {
    pub const ALL: [MouseButton; 5] = [
        MouseButton::Left,
        MouseButton::Middle,
        MouseButton::Right,
        MouseButton::Back,
        MouseButton::Forward,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "PascalCase")]
pub enum MouseAction {
    Move { x: f64, y: f64 },

    ButtonDown { button: MouseButton },

    ButtonUp { button: MouseButton },

    Wheel { delta_x: i64, delta_y: i64 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_move_with_type_tag() {
        let action = MouseAction::Move { x: 500.0, y: 400.0 };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains(r#""type":"Move""#), "{json}");
        assert!(json.contains(r#""x":500.0"#), "{json}");
        assert!(json.contains(r#""y":400.0"#), "{json}");
    }

    #[test]
    fn serializes_button_with_pascal_case_button_name() {
        let action = MouseAction::ButtonDown {
            button: MouseButton::Left,
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains(r#""type":"ButtonDown""#), "{json}");
        assert!(json.contains(r#""button":"Left""#), "{json}");
    }

    #[test]
    fn serializes_wheel_with_both_deltas() {
        let action = MouseAction::Wheel {
            delta_x: -3,
            delta_y: 5,
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains(r#""type":"Wheel""#), "{json}");
        assert!(json.contains(r#""delta_x":-3"#), "{json}");
        assert!(json.contains(r#""delta_y":5"#), "{json}");
    }

    #[test]
    fn move_roundtrip() {
        let action = MouseAction::Move {
            x: -320.5,
            y: 1080.0,
        };
        let json = serde_json::to_string(&action).unwrap();
        let parsed: MouseAction = serde_json::from_str(&json).unwrap();
        assert_eq!(action, parsed);
    }
}
