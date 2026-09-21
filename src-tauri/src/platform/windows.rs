/// Windows 滚轮换算。
///
/// rdev（录制）：WM_MOUSEWHEEL / WM_MOUSEHWHEEL 的 delta 除以
/// WHEEL_DELTA(120)，单位为格，正 delta_y = 向上、正 delta_x = 向右。
///
/// enigo（回放）：`scroll` 内部乘回 WHEEL_DELTA，单位同为格，
/// 垂直正 = 向下、水平正 = 向右。

/// Windows 原生一格滚轮的 delta 值。
pub const WHEEL_DELTA_NOTCH: i64 = 120;

pub fn wheel_vertical_to_enigo(delta_y: i64) -> i32 {
    clamp_i32(-delta_y)
}

pub fn wheel_horizontal_to_enigo(delta_x: i64) -> i32 {
    clamp_i32(delta_x)
}

fn clamp_i32(v: i64) -> i32 {
    v.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertical_sign_is_inverted() {
        // rdev 向上（+1 格）→ enigo 向下 1 格
        assert_eq!(wheel_vertical_to_enigo(1), -1);
        assert_eq!(wheel_vertical_to_enigo(-2), 2);
    }

    #[test]
    fn horizontal_sign_is_preserved() {
        assert_eq!(wheel_horizontal_to_enigo(1), 1);
        assert_eq!(wheel_horizontal_to_enigo(-2), -2);
    }
}
