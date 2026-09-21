#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod imp;

#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod imp;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
compile_error!("mouse-record only supports macOS and Windows");

/// 录制侧统一语义（rdev）：
/// - delta_y > 0 = 向上滚动
/// - delta_x > 0 = 向右滚动
///
/// 回放侧统一语义（enigo）：
/// - 垂直 length > 0 = 向下滚动
/// - 水平 length > 0 = 向右滚动
///
/// 本模块负责各平台的符号与单位换算。
pub use imp::*;
