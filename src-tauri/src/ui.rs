use tauri::{AppHandle, Emitter, Manager};

use crate::model::state::EngineState;

/// 主窗口 label（`tauri.conf.json` 未显式配置，使用 Tauri 默认值）。
const MAIN_WINDOW: &str = "main";

/// 引擎状态对外发布的唯一出口。
///
/// 录制/回放期间隐藏主窗口，避免遮挡目标应用，也让全局热键成为该阶段的操作入口；
/// 回到 Idle（正常结束、被停止或出错）时恢复显示。
/// 窗口尚不存在（例如启动早期）时静默跳过，不影响状态事件本身。
pub fn publish_state(app: &AppHandle, state: EngineState) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = if state == EngineState::Idle {
            window.show()
        } else {
            window.hide()
        };
    }

    let _ = app.emit("engine-state-changed", state);
}
