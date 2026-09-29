use tauri::{AppHandle, Emitter, Manager};

use crate::model::state::EngineState;
use crate::AppState;

/// 主窗口 label（`tauri.conf.json` 未显式配置，使用 Tauri 默认值）。
const MAIN_WINDOW: &str = "main";

/// 引擎状态对外发布的唯一出口。
///
/// 录制/回放期间隐藏主窗口，避免遮挡目标应用，也让全局热键成为该阶段的操作入口；
/// 回到 Idle（正常结束、被停止或出错）时恢复显示。
/// 窗口尚不存在（例如启动早期）时静默跳过，不影响状态事件本身。
pub fn publish_state(app: &AppHandle, state: EngineState) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = if should_hide(app, state) {
            window.hide()
        } else {
            window.show()
        };
    }

    let _ = app.emit("engine-state-changed", state);
}

/// 从任何入口把主窗口找回来并置前：Dock/任务栏点击、以及需要用户操作的兜底。
pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// 录制/回放期间隐藏窗口，但**输入监听不可用时绝不隐藏**：
/// 那时全局热键（隐藏期间唯一的停止入口）本来就用不了，
/// 再隐藏窗口会让用户既录不到东西、又停不下来。
fn should_hide(app: &AppHandle, state: EngineState) -> bool {
    if state == EngineState::Idle {
        return false;
    }

    listener_running(app)
}

fn listener_running(app: &AppHandle) -> bool {
    app.try_state::<AppState>()
        .map(|state| state.hub.is_listener_running())
        // 状态未注册（理论上不会发生）时按既有行为处理
        .unwrap_or(true)
}
