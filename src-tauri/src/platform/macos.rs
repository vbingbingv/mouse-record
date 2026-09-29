//! macOS 滚轮换算。
//!
//! 录制（cgtap）：读 CGEvent 的 point delta，单位为像素点，
//! 正 delta_y = 向上、正 delta_x = 向右。
//!
//! 回放（cgpost）：同为像素点，原样投递即与录制端精确对称，
//! 没有单位换算，只需要把 i64 夹取到 CGEvent 入参的 i32 范围。
//! （像素 → 行 → enigo 的换算只属于 Windows 的 enigo 后端，见 windows.rs。）

pub fn wheel_vertical_to_output(delta_y: i64) -> i32 {
    clamp_i32(delta_y)
}

pub fn wheel_horizontal_to_output(delta_x: i64) -> i32 {
    clamp_i32(delta_x)
}

fn clamp_i32(value: i64) -> i32 {
    value.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_are_preserved() {
        // 录制侧正 = 向上 / 向右，CGEvent 的 point delta 同向
        assert_eq!(wheel_vertical_to_output(30), 30);
        assert_eq!(wheel_vertical_to_output(-30), -30);
        assert_eq!(wheel_horizontal_to_output(25), 25);
        assert_eq!(wheel_horizontal_to_output(-25), -25);
    }

    #[test]
    fn zero_stays_zero() {
        assert_eq!(wheel_vertical_to_output(0), 0);
        assert_eq!(wheel_horizontal_to_output(0), 0);
    }

    #[test]
    fn out_of_range_deltas_are_clamped() {
        assert_eq!(wheel_vertical_to_output(i64::MAX), i32::MAX);
        assert_eq!(wheel_vertical_to_output(i64::MIN), i32::MIN);
        assert_eq!(wheel_horizontal_to_output(i64::MAX), i32::MAX);
    }
}
