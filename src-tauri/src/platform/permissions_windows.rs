//! Windows 权限实现：无需运行期申请。
//!
//! 提权由 `app.manifest` 的 `requireAdministrator` 在启动时完成（系统弹 UAC），
//! 监听与回放随之始终可用，因此这里恒为已授权 —— 也保证 Windows 侧行为不变。

use super::{PermissionKind, PermissionStatus};

pub fn platform_name() -> &'static str {
    "windows"
}

pub fn status() -> PermissionStatus {
    PermissionStatus {
        platform: platform_name(),
        listen_input: true,
        post_events: true,
    }
}

/// 与 `status` 相同：Windows 没有运行期可申请的权限。
pub fn request() -> PermissionStatus {
    status()
}

/// Windows 上没有对应的系统设置面板（提权在启动时由 UAC 完成）。
pub fn settings_url(_kind: PermissionKind) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_is_always_granted() {
        let current = status();
        assert!(current.listen_input && current.post_events);
        let requested = request();
        assert!(requested.listen_input && requested.post_events);
        assert_eq!(current.platform, "windows");
    }

    #[test]
    fn no_settings_panel() {
        assert!(settings_url(PermissionKind::ListenInput).is_none());
        assert!(settings_url(PermissionKind::PostEvents).is_none());
    }
}
