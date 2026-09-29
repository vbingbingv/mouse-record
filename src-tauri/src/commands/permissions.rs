use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

use crate::error::AutomationError;
use crate::platform::permissions::{self, PermissionKind, PermissionStatus};

/// 当前权限状态（macOS：TCC 两项；Windows：恒为已授权，见 platform::permissions）。
#[tauri::command]
pub fn get_permission_status() -> PermissionStatus {
    permissions::status()
}

/// 申请权限：macOS 上未决定过时会弹系统授权对话框，返回申请后的状态。
#[tauri::command]
pub fn request_permission() -> PermissionStatus {
    permissions::request()
}

/// 打开系统设置里对应的授权面板（用户拒绝过之后只能在这里手动打开开关）。
#[tauri::command]
pub fn open_permission_settings(
    app: AppHandle,
    kind: PermissionKind,
) -> Result<(), AutomationError> {
    let Some(url) = permissions::settings_url(kind) else {
        return Err(AutomationError::Unsupported(
            "this platform has no permission settings panel".into(),
        ));
    };

    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| AutomationError::InputBackend(format!("open permission settings: {e}")))
}

/// 重启应用。
///
/// macOS 的 TCC 判定对**已运行进程**有缓存：用户刚在系统设置里打开开关时，
/// 事件监听往往要等进程重启后才真正生效，所以给界面一个直接重启的入口。
#[tauri::command]
pub fn restart_app(app: AppHandle) {
    app.restart();
}
