#[cfg(target_os = "macos")]
pub mod cgpost;
#[cfg(target_os = "macos")]
pub mod cgtap;
pub mod controller;
#[cfg(target_os = "windows")]
pub mod enigo;
#[cfg(target_os = "windows")]
pub mod rdev;
pub mod source;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::error::AutomationError;
use controller::MouseController;
use source::MouseEventSource;

/// 按平台选择输入事件源。
///
/// macOS：自建 CGEventTap（rdev 0.5.3 在 macOS 上丢弃拖拽移动与中键事件）。
/// Windows：rdev（WM_MOUSEMOVE 在拖拽时照常触发，无此问题）。
pub fn platform_source(running: Arc<AtomicBool>) -> Box<dyn MouseEventSource> {
    #[cfg(target_os = "macos")]
    {
        Box::new(cgtap::CgEventTapSource::new(running))
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(rdev::RdevEventSource::new(running))
    }
}

/// 按平台选择回放用的鼠标控制器。
///
/// macOS：CGEvent 直投（enigo 0.2.1 在 macOS 上点击坐标错位，见 `cgpost` 模块说明）。
/// Windows：enigo（行为与既有版本完全一致）。
///
/// 控制器必须在回放线程内创建与使用：macOS 上 CGEventSource 不是 Send。
pub fn platform_controller() -> Result<Box<dyn MouseController>, AutomationError> {
    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(cgpost::CgEventPostController::new()?))
    }
    #[cfg(target_os = "windows")]
    {
        Ok(Box::new(enigo::EnigoMouseController::new()?))
    }
}
