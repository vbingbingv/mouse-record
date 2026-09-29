#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod imp;

#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod imp;

pub mod permissions;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
compile_error!("mouse-record only supports macOS and Windows");

/// 录制侧统一语义（rdev / CGEvent）：
/// - delta_y > 0 = 向上滚动
/// - delta_x > 0 = 向右滚动
///
/// 回放侧各平台不同，由本模块负责换算：
/// - Windows（enigo）：单位为格，垂直正 = 向下 → 垂直取反、水平直通；
/// - macOS（CGEvent）：单位同为像素 point delta，正 = 向上 / 向右 → 原样直通。
pub use imp::*;
