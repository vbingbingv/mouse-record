use std::sync::Arc;
use std::time::Instant;

use crate::error::AutomationError;
use crate::model::action::MouseAction;

/// 原始输入事件（不直接持久化，由 Recorder 决定取舍）。
///
/// Mouse 变体携带事件到达时刻：在事件源（rdev 回调线程）打点，
/// 经过 channel 排队的延迟不会扭曲录制时间。
#[derive(Debug, Clone, Copy)]
pub enum RawInputEvent {
    Mouse(RawMouseEvent),

    /// 目前唯一关心的键盘事件：ESC（紧急停止）。
    EscapePressed,

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
