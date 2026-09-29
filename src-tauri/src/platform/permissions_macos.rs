//! macOS 权限实现（TCC）。
//!
//! `CGPreflight*EventAccess` / `CGRequest*EventAccess` 是 macOS 10.15 起提供的
//! TCC 查询与申请接口：preflight 只查询，request 在用户尚未决定时会弹出系统对话框
//! （每个应用只弹一次），之后只能由用户在系统设置里改动。
//! 因此 `bundle.macOS.minimumSystemVersion` 声明为 10.15。

use super::{PermissionKind, PermissionStatus};

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    /// 是否有权限监听事件（“输入监控”）。
    fn CGPreflightListenEventAccess() -> bool;
    fn CGRequestListenEventAccess() -> bool;

    /// 是否有权限合成事件（“辅助功能”）。
    fn CGPreflightPostEventAccess() -> bool;
    fn CGRequestPostEventAccess() -> bool;
}

pub fn platform_name() -> &'static str {
    "macos"
}

/// 只查询当前授权状态，不弹任何对话框。
pub fn status() -> PermissionStatus {
    unsafe {
        PermissionStatus {
            platform: platform_name(),
            listen_input: CGPreflightListenEventAccess(),
            post_events: CGPreflightPostEventAccess(),
        }
    }
}

/// 申请授权：未决定时弹系统对话框（用户拒绝过则只返回 false，不会再弹），
/// 返回申请后的状态，等价于 Windows 启动时的 UAC 弹窗。
pub fn request() -> PermissionStatus {
    unsafe {
        PermissionStatus {
            platform: platform_name(),
            listen_input: CGRequestListenEventAccess(),
            post_events: CGRequestPostEventAccess(),
        }
    }
}

/// 打开对应的系统设置面板（用户拒绝过之后只能在这里手动打开开关）。
pub fn settings_url(kind: PermissionKind) -> Option<String> {
    let pane = match kind {
        PermissionKind::ListenInput => "Privacy_ListenEvent",
        PermissionKind::PostEvents => "Privacy_Accessibility",
    };
    Some(format!(
        "x-apple.systempreferences:com.apple.preference.security?{pane}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_urls_point_at_the_matching_pane() {
        assert_eq!(
            settings_url(PermissionKind::ListenInput).unwrap(),
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent"
        );
        assert_eq!(
            settings_url(PermissionKind::PostEvents).unwrap(),
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        );
    }

    #[test]
    fn status_reports_platform_name() {
        assert_eq!(status().platform, "macos");
    }
}
