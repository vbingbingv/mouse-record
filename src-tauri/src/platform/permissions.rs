//! 平台权限：macOS 的 TCC 授权（输入监控 / 辅助功能），Windows 的 UAC 提权。
//!
//! Windows 用 `app.manifest` 的 `requireAdministrator` 在启动时由系统弹 UAC，
//! 没有运行期可申请的权限，所以这里的实现恒为“已授权”，行为与既有版本一致。
//!
//! macOS 需要在系统设置里显式授权，且分两项：
//! - 输入监控（Input Monitoring）：创建 listen-only 的 CGEventTap，录制与全局热键都依赖它；
//! - 辅助功能（Accessibility）：投递合成事件，回放依赖它。
//!
//! 未授权时事件源创建失败 / 事件被静默丢弃，因此需要在界面上引导用户授权。

#[cfg(target_os = "macos")]
#[path = "permissions_macos.rs"]
mod imp;

#[cfg(target_os = "windows")]
#[path = "permissions_windows.rs"]
mod imp;

pub use imp::{request, settings_url, status};

use serde::{Deserialize, Serialize};

/// 需要引导用户授权的权限项。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionKind {
    /// 监听全局输入（录制 + 全局热键）。
    ListenInput,
    /// 合成鼠标事件（回放）。
    PostEvents,
}

/// 当前权限状态。Windows 上恒为全部已授权。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PermissionStatus {
    pub platform: &'static str,

    /// 监听全局输入是否已授权。
    pub listen_input: bool,

    /// 合成鼠标事件是否已授权。
    pub post_events: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_platform_and_flags_for_the_frontend() {
        let status = PermissionStatus {
            platform: "macos",
            listen_input: true,
            post_events: false,
        };
        assert_eq!(
            serde_json::to_string(&status).unwrap(),
            r#"{"platform":"macos","listen_input":true,"post_events":false}"#
        );
    }

    #[test]
    fn kind_serializes_camel_case() {
        assert_eq!(
            serde_json::to_string(&PermissionKind::ListenInput).unwrap(),
            "\"listenInput\""
        );
        assert_eq!(
            serde_json::to_string(&PermissionKind::PostEvents).unwrap(),
            "\"postEvents\""
        );
        assert_eq!(
            serde_json::from_str::<PermissionKind>("\"postEvents\"").unwrap(),
            PermissionKind::PostEvents
        );
    }
}
