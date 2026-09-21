#[cfg(target_os = "macos")]
pub mod cgtap;
pub mod controller;
pub mod enigo;
#[cfg(target_os = "windows")]
pub mod rdev;
pub mod source;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

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
