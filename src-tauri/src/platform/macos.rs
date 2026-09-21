/// macOS 滚轮换算。
///
/// rdev（录制）：报告的是 CGEvent 的 point delta，单位为像素，
/// 正 delta_y = 向上、正 delta_x = 向右。
///
/// enigo（回放）：`scroll` 以行为单位（ScrollEventUnit::LINE），
/// 垂直正 = 向下、水平正 = 向右。
///
/// core-graphics crate 未暴露 CGEventSourcePixelsPerLine，
/// 因此用可校准的启发式常量换算像素 → 行。

/// 默认每行对应的滚动像素数（macOS 默认约 10px/行，可按手感调整）。
pub const MACOS_PIXELS_PER_LINE: f64 = 10.0;

pub fn wheel_vertical_to_enigo(delta_y: i64) -> i32 {
    // rdev 正 = 向上；enigo 垂直正 = 向下 → 取反。
    // 先按原始方向舍入再取反，保证正负两侧舍入对称。
    -pixels_to_lines(delta_y)
}

pub fn wheel_horizontal_to_enigo(delta_x: i64) -> i32 {
    // 两侧均为正 = 向右，仅换单位
    pixels_to_lines(delta_x)
}

fn pixels_to_lines(delta_px: i64) -> i32 {
    let lines = delta_px as f64 / MACOS_PIXELS_PER_LINE;
    lines.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertical_sign_is_inverted() {
        // rdev 向上（+30px）→ enigo 向上（负值）3 行
        assert_eq!(wheel_vertical_to_enigo(30), -3);
        // rdev 向下（-30px）→ enigo 向下（正值）3 行
        assert_eq!(wheel_vertical_to_enigo(-30), 3);
    }

    #[test]
    fn horizontal_sign_is_preserved() {
        assert_eq!(wheel_horizontal_to_enigo(25), 3);
        assert_eq!(wheel_horizontal_to_enigo(-25), -3);
    }

    #[test]
    fn small_deltas_round_symmetrically() {
        assert_eq!(wheel_vertical_to_enigo(4), 0);
        assert_eq!(wheel_vertical_to_enigo(-4), 0);
        assert_eq!(wheel_vertical_to_enigo(5), -1);
        assert_eq!(wheel_vertical_to_enigo(-5), 1);
        assert_eq!(wheel_vertical_to_enigo(15), -2);
        assert_eq!(wheel_vertical_to_enigo(-15), 2);
    }
}
