use std::sync::Arc;
use std::time::Instant;

use crate::error::AutomationError;
use crate::model::action::MouseAction;

/// 全局功能热键。基础键是数字键，需要配合 Ctrl 使用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hotkey {
    /// Ctrl+8：开始/结束录制。
    ToggleRecording,
    /// Ctrl+9：开始/结束回放。
    ToggleReplay,
}

impl Hotkey {
    fn slot(self) -> usize {
        match self {
            Hotkey::ToggleRecording => 0,
            Hotkey::ToggleReplay => 1,
        }
    }
}

/// 按下沿去抖。
///
/// Windows 的键盘 auto-repeat 会在按住期间连续发 KeyPress 而不夹 KeyRelease，
/// 直接转发会让用户按住热键时反复开关；这里只放行“按下沿”，
/// 收到释放后才允许下一次触发。
#[derive(Debug, Default)]
pub struct HotkeyDebouncer {
    down: [bool; 2],
}

impl HotkeyDebouncer {
    /// 记录一次按下；返回 true 表示这是新的按下沿。
    pub fn on_press(&mut self, hotkey: Hotkey) -> bool {
        let down = &mut self.down[hotkey.slot()];
        let is_new = !*down;
        *down = true;
        is_new
    }

    /// 记录一次释放。
    pub fn on_release(&mut self, hotkey: Hotkey) {
        self.down[hotkey.slot()] = false;
    }
}

/// 热键过滤器：维护 Ctrl 状态，再叠加基础键的按下沿去抖。
///
/// 热键是 Ctrl+数字，只看基础键不够，必须自己跟踪 Ctrl 是否按住。
#[derive(Debug, Default)]
pub struct HotkeyFilter {
    debouncer: HotkeyDebouncer,
    control: bool,
}

impl HotkeyFilter {
    /// Ctrl 按下/释放。左右两侧不区分，任意一侧按住即算按下。
    pub fn on_ctrl(&mut self, down: bool) {
        self.control = down;
    }

    /// 基础键按下：满足 Ctrl 且处于按下沿时返回热键。
    ///
    /// 没按 Ctrl 时同样会记下"已按下"，因此先按住数字键再补 Ctrl
    /// 不会误触发，必须松开重按（避免按住数字键时随便加 Ctrl 就开录）。
    pub fn on_press(&mut self, hotkey: Hotkey) -> Option<Hotkey> {
        let is_press_edge = self.debouncer.on_press(hotkey);
        (is_press_edge && self.control).then_some(hotkey)
    }

    /// 基础键释放：只复位去抖状态。
    pub fn on_release(&mut self, hotkey: Hotkey) {
        self.debouncer.on_release(hotkey);
    }
}

/// 原始输入事件（不直接持久化，由 Recorder 决定取舍）。
///
/// Mouse 变体携带事件到达时刻：在事件源（rdev 回调线程）打点，
/// 经过 channel 排队的延迟不会扭曲录制时间。
#[derive(Debug, Clone, Copy)]
pub enum RawInputEvent {
    Mouse(RawMouseEvent),

    /// 功能热键（Ctrl+8/9）：由上层决定开始/结束录制或回放。
    Hotkey(Hotkey),

    Other,
}

/// 原始鼠标事件 + 到达时间。
#[derive(Debug, Clone, Copy)]
pub struct RawMouseEvent {
    pub time: Instant,
    pub action: MouseAction,
}

pub type EventSink = Arc<dyn Fn(RawInputEvent) + Send + Sync>;

/// 全局输入事件源抽象。
///
/// 实现为常驻监听：`listen_forever` 阻塞当前线程直到监听退出
/// （失败或被系统终止）。策略上不在录制停止时销毁事件源，
/// 避免平台层反复创建/销毁全局钩子带来的不稳定。
pub trait MouseEventSource: Send + Sync {
    fn listen_forever(&self, sink: EventSink) -> Result<(), AutomationError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debouncer_only_fires_on_press_edge() {
        let mut debouncer = HotkeyDebouncer::default();

        assert!(debouncer.on_press(Hotkey::ToggleRecording));
        // auto-repeat：没有释放就再次按下，应被忽略
        assert!(!debouncer.on_press(Hotkey::ToggleRecording));

        debouncer.on_release(Hotkey::ToggleRecording);
        assert!(debouncer.on_press(Hotkey::ToggleRecording));
    }

    #[test]
    fn debouncer_tracks_each_hotkey_independently() {
        let mut debouncer = HotkeyDebouncer::default();

        assert!(debouncer.on_press(Hotkey::ToggleRecording));
        assert!(debouncer.on_press(Hotkey::ToggleReplay));
        debouncer.on_release(Hotkey::ToggleReplay);

        // 释放过的可以再次触发，仍按住的不可
        assert!(debouncer.on_press(Hotkey::ToggleReplay));
        assert!(!debouncer.on_press(Hotkey::ToggleRecording));
    }

    #[test]
    fn filter_requires_ctrl() {
        let mut filter = HotkeyFilter::default();

        // 只按基础键：不触发
        assert_eq!(filter.on_press(Hotkey::ToggleRecording), None);
        filter.on_release(Hotkey::ToggleRecording);

        // Ctrl：触发
        filter.on_ctrl(true);
        assert_eq!(
            filter.on_press(Hotkey::ToggleRecording),
            Some(Hotkey::ToggleRecording)
        );
        // auto-repeat：还没松开就不重复触发
        assert_eq!(filter.on_press(Hotkey::ToggleRecording), None);

        // 松开 Ctrl 后再按也不再触发
        filter.on_release(Hotkey::ToggleRecording);
        filter.on_ctrl(false);
        assert_eq!(filter.on_press(Hotkey::ToggleRecording), None);
    }

    #[test]
    fn filter_ignores_combo_completed_after_key_down() {
        let mut filter = HotkeyFilter::default();

        // 先按住 9，再补 Ctrl：不应触发
        assert_eq!(filter.on_press(Hotkey::ToggleReplay), None);
        filter.on_ctrl(true);
        assert_eq!(filter.on_press(Hotkey::ToggleReplay), None);

        // 松开重按才算一次组合键
        filter.on_release(Hotkey::ToggleReplay);
        assert_eq!(
            filter.on_press(Hotkey::ToggleReplay),
            Some(Hotkey::ToggleReplay)
        );
    }
}
