# 鼠标行为录制与循环回放工具

## 1. 项目目标

这是一个基于 **Tauri + Rust + 前端 UI** 的桌面自动化工具。

核心目标：

> 捕捉用户真实鼠标行为，并按照录制时的行为和时间节奏自动重复执行，从而让用户可以把重复性的鼠标操作交给电脑完成。

第一阶段只实现：

* 全局鼠标行为录制
* 鼠标移动录制
* 鼠标左/中/右键按下与释放
* 点击行为
* 拖拽行为
* 滚轮行为
* 按原始时间节奏回放
* 循环回放
* 停止/紧急停止
* 录制数据持久化
* 基础前端可视化

### 第一阶段明确不实现

暂时不要实现以下能力：

* 虚拟鼠标 / 第二鼠标指针
* Accessibility / Windows UI Automation
* macOS Accessibility Tree
* OCR
* 图像识别
* AI 视觉识别
* 自动寻找按钮
* 自动适配窗口位置
* Web DOM 自动化
* 键盘录制
* 条件判断
* 变量系统
* 工作流编排

第一阶段的核心原则：

> **只模拟用户真实鼠标行为。**

因此回放过程中允许真实系统鼠标发生移动。

---

# 2. 产品形态

产品主要由三个部分组成：

```text
┌─────────────────────────────────────┐
│              Tauri App              │
│                                     │
│         React / Frontend UI         │
│                                     │
│   录制 / 停止 / 回放 / 循环次数      │
│   动作列表 / 时间线 / 当前状态       │
└────────────────┬────────────────────┘
                 │
                 │ Tauri IPC
                 ▼
┌─────────────────────────────────────┐
│             Rust Core               │
│                                     │
│  Recorder      ReplayEngine         │
│      │               │              │
│      ▼               ▼              │
│    rdev            enigo            │
│      │               │              │
│      └─────── OS ────┘              │
└─────────────────────────────────────┘
```

---

# 3. 技术栈

推荐：

```text
Frontend
- React
- TypeScript
- 现有项目使用的 UI 框架

Desktop
- Tauri 2

Rust
- Rust stable

Input
- rdev：全局鼠标事件监听
- enigo：鼠标事件模拟

Serialization
- serde
- serde_json

Async / Timing
- tokio

Error
- anyhow
- thiserror
```

注意：

> 不要盲目假设第三方 crate 的 API 与历史版本完全一致。实现时应以项目当前 Cargo.lock / 官方文档中的实际 API 为准。

---

# 4. 核心设计原则

## 4.1 录制原始事件，而不是过早抽象成“点击/拖拽”

不要把：

```text
ButtonDown
Move
Move
Move
ButtonUp
```

强行在录制阶段转换成：

```text
Drag
```

应该记录原始鼠标事件。

原因：

1. 拖拽天然就是 ButtonDown + MouseMove + ButtonUp
2. 可以保留完整轨迹
3. 可以支持绘图、滑块、复杂拖拽
4. 后续可以在 Replay 层再做行为分析
5. 避免第一阶段丢失信息

因此：

```text
Drag
```

不是底层基本事件。

底层基本事件只有：

```text
Move
ButtonDown
ButtonUp
Wheel
```

---

# 5. Action 数据模型

建议设计自己的领域模型，不要直接把 `rdev::Event` 作为持久化数据结构。

示例：

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    Back,
    Forward,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum MouseAction {
    Move {
        x: f64,
        y: f64,
    },

    ButtonDown {
        button: MouseButton,
    },

    ButtonUp {
        button: MouseButton,
    },

    Wheel {
        delta_x: i64,
        delta_y: i64,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RecordedAction {
    /// 距离上一个已记录 Action 的时间
    pub delay_ms: u64,

    pub action: MouseAction,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Recording {
    pub version: u32,

    pub actions: Vec<RecordedAction>,
}
```

---

# 6. 为什么要保存 delay，而不是保存绝对 timestamp

不要保存：

```rust
timestamp: 172123123123
```

作为回放核心依据。

推荐保存：

```rust
delay_ms
```

例如：

```text
Move
delay 120ms
ButtonDown
delay 30ms
Move
delay 20ms
Move
delay 50ms
ButtonUp
```

这样 ReplayEngine 很容易执行：

```rust
sleep(delay_ms)
execute(action)
```

并且录制文件更容易理解和编辑。

---

# 7. 多显示器坐标

鼠标位置使用系统绝对坐标。

不要自行约束：

```text
x >= 0
y >= 0
```

因为多显示器环境可能存在负坐标，例如：

```text
Monitor B
x < 0
```

因此：

```rust
x: f64,
y: f64,
```

必须允许负数。

不要在录制时根据当前主显示器宽高对坐标做归一化。

第一阶段先保持：

> 录什么坐标，回放什么坐标。

---

# 8. Recorder

## 8.1 Recorder 职责

Recorder 负责：

1. 开始监听全局鼠标事件
2. 转换为业务 Action
3. 记录事件时间
4. 对 MouseMove 做合理压缩
5. 写入内存 buffer
6. 向前端发送必要的状态更新
7. 停止录制
8. 输出 `Recording`

---

# 9. Recorder 状态

建议：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineState {
    Idle,
    Recording,
    Replaying,
    Stopping,
}
```

状态必须保证：

```text
Idle
  ↓
Recording
  ↓
Idle

Idle
  ↓
Replaying
  ↓
Idle
```

禁止：

```text
Recording -> Recording

Recording -> Replaying

Replaying -> Recording
```

除非后续明确设计状态迁移。

---

# 10. Recorder 线程模型

`rdev::listen` 属于持续监听型 API，不要直接阻塞 Tauri command 主执行线程。

建议：

```text
Tauri Command
     │
     ▼
Recorder.start()
     │
     ▼
spawn dedicated thread
     │
     ▼
rdev::listen(callback)
```

回调只负责快速处理事件。

不要在鼠标 callback 里面：

* 做耗时计算
* 写磁盘
* 调用网络
* 执行复杂 UI 操作
* 大量发送 Tauri IPC

推荐：

```text
OS event
   ↓
rdev callback
   ↓
normalize event
   ↓
channel
   ↓
Recorder worker
   ↓
memory buffer
```

---

# 11. MouseMove 压缩

这是第一阶段必须实现的优化。

用户移动鼠标时可能产生大量 MouseMove：

```text
Move
Move
Move
Move
Move
Move
...
```

如果全部记录：

* 文件会非常大
* 回放时 IPC/执行次数很多
* 对简单鼠标操作没有实际意义

推荐基本策略：

## 普通移动

没有任何鼠标按钮按下时：

只有达到一定距离或时间阈值才保存。

例如：

```text
distance >= 3px
OR
elapsed >= 20ms
```

具体阈值允许做成常量。

---

## 拖拽移动

如果存在任意 ButtonDown 状态：

```rust
button_down == true
```

则应尽量保留 MouseMove。

例如：

```text
ButtonDown
Move
Move
Move
Move
ButtonUp
```

不能过度压缩成：

```text
ButtonDown
Move(start -> end)
ButtonUp
```

第一阶段应优先保证行为准确性。

---

# 12. 鼠标按钮状态

Recorder 必须维护：

```rust
struct MouseButtonState {
    left: bool,
    middle: bool,
    right: bool,
    back: bool,
    forward: bool,
}
```

事件：

```text
ButtonDown
```

对应：

```rust
state.left = true;
```

事件：

```text
ButtonUp
```

对应：

```rust
state.left = false;
```

必须允许多个按钮同时处于按下状态。

例如：

```text
Left Down
Move
Right Down
Move
Left Up
Right Up
```

不要假设一次只能按一个键。

---

# 13. Wheel

Wheel 事件必须保留：

```rust
delta_x
delta_y
```

不要只保存：

```rust
direction = Up / Down
```

因为需要支持：

* 垂直滚动
* 水平滚动
* 不同滚动幅度

示例：

```rust
MouseAction::Wheel {
    delta_x,
    delta_y,
}
```

回放时通过 `enigo` 对应 Axis 执行。

注意：

> 不同操作系统 / 输入设备的滚轮 delta 语义可能存在差异。

第一阶段：

> 优先做到“在同一台机器录制和回放时行为一致”。

不要在第一版试图做跨设备滚轮行为的数学归一化。

---

# 14. Click 不作为最底层事件

点击应该被理解为：

```text
ButtonDown
+
ButtonUp
```

不要强制把底层数据改成：

```text
Click
```

否则容易丢失：

* 长按
* 拖拽
* ButtonDown 后等待
* ButtonDown 后移动

例如：

```text
ButtonDown
delay 1500ms
ButtonUp
```

这不是普通 Click，而是长按。

因此底层仍然保存：

```text
ButtonDown
ButtonUp
```

前端可以根据规则展示：

```text
Click
Long Press
Drag
```

但内部原始数据仍然保持不变。

---

# 15. ReplayEngine

ReplayEngine 的职责：

1. 接收一个 `Recording`
2. 根据 `delay_ms` 等待
3. 执行鼠标 Action
4. 支持循环
5. 支持停止
6. 防止回放事件被 Recorder 再次记录

基本伪代码：

```rust
pub async fn replay(
    recording: Recording,
    loops: u32,
    stop_flag: Arc<AtomicBool>,
) -> Result<()> {

    for loop_index in 0..loops {
        for item in &recording.actions {

            if stop_flag.load(Ordering::Relaxed) {
                emergency_release_buttons();
                return Ok(());
            }

            sleep(Duration::from_millis(item.delay_ms)).await;

            if stop_flag.load(Ordering::Relaxed) {
                emergency_release_buttons();
                return Ok(());
            }

            execute_action(&item.action)?;
        }
    }

    Ok(())
}
```

---

# 16. Action Replay

伪代码：

```rust
fn execute_action(
    action: &MouseAction,
    enigo: &mut Enigo,
) -> Result<()> {

    match action {

        MouseAction::Move { x, y } => {
            enigo.move_mouse(
                *x as i32,
                *y as i32,
                Coordinate::Abs,
            )?;
        }

        MouseAction::ButtonDown { button } => {
            enigo.button(
                to_enigo_button(button),
                Direction::Press,
            )?;
        }

        MouseAction::ButtonUp { button } => {
            enigo.button(
                to_enigo_button(button),
                Direction::Release,
            )?;
        }

        MouseAction::Wheel {
            delta_x,
            delta_y,
        } => {
            if *delta_y != 0 {
                enigo.scroll(
                    *delta_y as i32,
                    Axis::Vertical,
                )?;
            }

            if *delta_x != 0 {
                enigo.scroll(
                    *delta_x as i32,
                    Axis::Horizontal,
                )?;
            }
        }
    }

    Ok(())
}
```

实现时以当前 `enigo` API 为准，不要复制历史版本 API。

---

# 17. 回放过程中禁止再次录制

这是必须解决的问题。

错误流程：

```text
ReplayEngine
     ↓
Enigo
     ↓
系统产生 MouseMove
     ↓
rdev 捕获
     ↓
Recorder 保存
```

这样会导致录制数据被污染。

因此：

```rust
if state != EngineState::Recording {
    ignore_recording_event();
}
```

同时 Replay 开始前将状态切换：

```text
Idle
 ↓
Replaying
```

Replay 结束：

```text
Replaying
 ↓
Idle
```

不要通过简单的“Replay 时暂停 Recorder”来解决，必须由统一状态管理。

---

# 18. 紧急停止

这是核心安全功能，不是可选功能。

因为自动鼠标一旦进入错误状态，用户必须可以马上终止。

推荐设计：

```text
ESC
```

作为紧急停止键。

也可以额外支持配置。

ReplayEngine 必须检查：

```rust
stop_flag
```

而不是等当前动作执行完才停止。

---

# 19. 鼠标按键异常恢复

极端情况：

```text
ButtonDown
Move
程序崩溃
```

可能导致鼠标逻辑处于异常状态。

因此：

```rust
fn emergency_release_buttons() {
    // 对当前可能处于 pressed 状态的鼠标按钮执行 Release
}
```

ReplayEngine 自己需要维护：

```rust
ReplayButtonState
```

例如：

```rust
struct ReplayButtonState {
    left: bool,
    middle: bool,
    right: bool,
    back: bool,
    forward: bool,
}
```

在退出 replay、stop、panic cleanup 等场景中尽可能释放仍然被按下的按钮。

---

# 20. 前端与 Rust 的通信

Tauri Command：

```text
Frontend
   ↓
invoke("start_recording")
invoke("stop_recording")
invoke("start_replay")
invoke("stop_replay")
invoke("save_recording")
invoke("load_recording")
```

推荐 command：

```rust
start_recording()
stop_recording()
start_replay(recording, options)
stop_replay()
get_engine_state()
save_recording(recording, path)
load_recording(path)
```

Rust → Frontend 使用 Tauri Event。

可以发送：

```text
recording-started
recording-stopped
recording-action
replay-started
replay-progress
replay-finished
replay-stopped
engine-state-changed
```

---

# 21. 不要高频把 MouseMove 发送到 JS

不要：

```text
Rust MouseMove
 ↓
Tauri Event
 ↓
React
 ↓
setState()
```

每一个 MouseMove 都同步到 UI。

这样会造成：

```text
OS
 ↓
Rust
 ↓
IPC
 ↓
JS
 ↓
React render
```

高频压力。

推荐：

```text
Rust
 ↓
持续收集
 ↓
低频发送 UI 状态
```

例如：

```text
当前录制时长
Action 数量
当前鼠标位置
最近一个 Action
```

前端不需要看到所有原始 MouseMove。

---

# 22. 前端 UI

第一版不需要复杂。

建议包含：

```text
┌─────────────────────────────────────┐
│         Mouse Automation            │
├─────────────────────────────────────┤
│                                     │
│ 状态：未录制                        │
│                                     │
│ [开始录制] [停止录制]               │
│                                     │
│ ─────────────────────────────────── │
│                                     │
│ 动作数量：128                       │
│ 录制时长：00:01:24                   │
│                                     │
│ ┌───────────────────────────────┐   │
│ │ Move                         │   │
│ │ Left Down                    │   │
│ │ Move                         │   │
│ │ Move                         │   │
│ │ Left Up                      │   │
│ │ Wheel Down                   │   │
│ └───────────────────────────────┘   │
│                                     │
│ 循环次数： [ 10 ]                    │
│ 每次循环间隔： [ 500 ] ms            │
│                                     │
│ [开始回放] [停止回放]                │
│                                     │
└─────────────────────────────────────┘
```

---

# 23. Action Timeline

前端不要直接展示原始 JSON。

应该把 Action 转换成人类可读描述。

例如：

```text
00:00.000  Move              (420, 500)
00:00.420  Left Button Down
00:00.450  Move              (430, 510)
00:00.470  Move              (450, 530)
00:00.500  Move              (500, 580)
00:00.530  Left Button Up
00:01.200  Wheel              ↓ 5
```

以后可以演进成：

```text
00:00.000  移动鼠标
00:00.420  按下左键
00:00.530  拖拽
00:01.200  向下滚动
```

---

# 24. 录制文件

第一版直接使用 JSON。

例如：

```json
{
  "version": 1,
  "actions": [
    {
      "delay_ms": 0,
      "action": {
        "type": "Move",
        "x": 500,
        "y": 400
      }
    },
    {
      "delay_ms": 200,
      "action": {
        "type": "ButtonDown",
        "button": "Left"
      }
    },
    {
      "delay_ms": 20,
      "action": {
        "type": "Move",
        "x": 520,
        "y": 420
      }
    },
    {
      "delay_ms": 20,
      "action": {
        "type": "Move",
        "x": 550,
        "y": 450
      }
    },
    {
      "delay_ms": 20,
      "action": {
        "type": "ButtonUp",
        "button": "Left"
      }
    }
  ]
}
```

---

# 25. Recording version

必须保留：

```rust
version: u32
```

例如：

```text
version = 1
```

因为未来很可能出现：

```text
version 2
增加 keyboard
version 3
增加 window
version 4
增加 semantic target
```

不要假设 JSON schema 永远不变。

---

# 26. 未来可扩展 Action

当前：

```rust
MouseAction
```

未来可以升级成：

```rust
pub enum Action {
    Mouse(MouseAction),
    Keyboard(KeyboardAction),
    Wait(WaitAction),
    Screenshot(...),
    Window(...),
}
```

但第一阶段不要提前实现。

目标是让现在的鼠标模型未来容易扩展，而不是提前把整个 RPA 系统做出来。

---

# 27. Drag 行为

第一阶段无需单独的：

```rust
Drag {
    from,
    to,
}
```

因为：

```text
ButtonDown
Move*
ButtonUp
```

天然表示拖拽。

可以在展示层识别：

```text
ButtonDown
+
连续 Move
+
ButtonUp
```

展示为：

```text
Drag
```

但内部仍然保存原始事件。

---

# 28. Long Press

同理，不要设计成：

```text
LongPress
```

底层保存：

```text
ButtonDown
delay = 1500ms
ButtonUp
```

前端可以推导：

```text
duration > 500ms
```

显示：

```text
长按 1.5s
```

---

# 29. Double Click

不要强制保存：

```text
DoubleClick
```

底层保存：

```text
ButtonDown
ButtonUp
ButtonDown
ButtonUp
```

这样可以完整还原真实用户事件。

UI 层可以把短间隔的两个 Click 聚合展示为：

```text
Double Click
```

---

# 30. Replay 速度

第一阶段建议支持：

```text
Replay Speed = 1.0x
```

数据模型最好不要绑定具体 speed。

可以在 ReplayEngine 层计算：

```rust
effective_delay =
    original_delay_ms / speed
```

以后即可支持：

```text
0.5x
1x
2x
5x
```

注意：

> 速度调整只改变 delay，不应该修改鼠标坐标或事件顺序。

---

# 31. Replay 模式

建议定义：

```rust
pub struct ReplayOptions {
    pub loops: u32,
    pub loop_interval_ms: u64,
    pub speed: f64,
}
```

默认：

```text
loops = 1
loop_interval_ms = 0
speed = 1.0
```

---

# 32. 时间精度

不要依赖：

```text
sleep(1ms)
```

就认为系统一定精确等待 1ms。

操作系统调度存在误差。

第一阶段目标：

> 行为时序足够接近录制结果，而不是追求实时系统级的微秒级精度。

对于一般：

* 办公软件
* 浏览器
* 文件管理
* 后台管理系统
* 普通 GUI

毫秒级时间误差通常不是核心问题。

---

# 33. 错误处理

不要让错误直接 panic。

例如：

```rust
pub enum AutomationError {
    AlreadyRecording,
    NotRecording,
    AlreadyReplaying,
    NotReplaying,
    InputBackendError(String),
    InvalidRecording(String),
    UnsupportedPlatform,
    ReplayStopped,
}
```

推荐：

```rust
Result<T, AutomationError>
```

Tauri command 再把错误转换成前端友好的结构。

---

# 34. 平台支持

第一阶段优先保证：

```text
macOS
Windows
```

Linux 暂时不作为第一验收目标。

原因：

* 全局输入监听存在平台差异
* X11 / Wayland 行为差异
* 权限模型差异
* 输入注入能力差异

必须把平台相关代码隔离：

```text
platform/
├── mod.rs
├── macos.rs
├── windows.rs
└── linux.rs
```

不要在 Recorder 和 ReplayEngine 中到处写：

```rust
#[cfg(target_os = "...")]
```

平台差异尽可能封装在 adapter 层。

---

# 35. 推荐抽象

不要让核心业务逻辑直接依赖 `rdev` 和 `enigo`。

建议定义接口：

```rust
pub trait MouseEventSource {
    fn start(
        &self,
        sender: Sender<RawMouseEvent>,
    ) -> Result<()>;

    fn stop(&self) -> Result<()>;
}
```

回放：

```rust
pub trait MouseController {
    fn move_to(&mut self, x: f64, y: f64) -> Result<()>;

    fn button_down(
        &mut self,
        button: MouseButton,
    ) -> Result<()>;

    fn button_up(
        &mut self,
        button: MouseButton,
    ) -> Result<()>;

    fn scroll(
        &mut self,
        delta_x: i64,
        delta_y: i64,
    ) -> Result<()>;
}
```

这样：

```text
rdev
  ↓
RdevMouseEventSource

enigo
  ↓
EnigoMouseController
```

以后如果更换底层库，不需要重写 Recorder / ReplayEngine。

---

# 36. 推荐目录结构

```text
src-tauri/
├── src/
│   ├── lib.rs
│   │
│   ├── commands/
│   │   ├── mod.rs
│   │   ├── recording.rs
│   │   └── replay.rs
│   │
│   ├── model/
│   │   ├── mod.rs
│   │   ├── action.rs
│   │   └── recording.rs
│   │
│   ├── recorder/
│   │   ├── mod.rs
│   │   ├── recorder.rs
│   │   ├── state.rs
│   │   └── normalize.rs
│   │
│   ├── replay/
│   │   ├── mod.rs
│   │   ├── engine.rs
│   │   └── state.rs
│   │
│   ├── input/
│   │   ├── mod.rs
│   │   ├── source.rs
│   │   └── controller.rs
│   │
│   ├── platform/
│   │   ├── mod.rs
│   │   ├── macos.rs
│   │   ├── windows.rs
│   │   └── linux.rs
│   │
│   └── error.rs
│
└── Cargo.toml
```

前端：

```text
src/
├── pages/
│   └── automation/
│       ├── index.tsx
│       ├── ActionTimeline.tsx
│       ├── RecordControls.tsx
│       └── ReplayControls.tsx
│
├── hooks/
│   └── useAutomation.ts
│
├── api/
│   └── automation.ts
│
└── types/
    └── automation.ts
```

---

# 37. Tauri Commands

第一版推荐：

```rust
#[tauri::command]
async fn start_recording() -> Result<(), String>;

#[tauri::command]
async fn stop_recording() -> Result<Recording, String>;

#[tauri::command]
async fn start_replay(
    recording: Recording,
    options: ReplayOptions,
) -> Result<(), String>;

#[tauri::command]
async fn stop_replay() -> Result<(), String>;

#[tauri::command]
async fn get_engine_state() -> Result<EngineState, String>;
```

前端 API：

```ts
export async function startRecording() {
  return invoke('start_recording');
}

export async function stopRecording() {
  return invoke<Recording>('stop_recording');
}

export async function startReplay(
  recording: Recording,
  options: ReplayOptions,
) {
  return invoke('start_replay', {
    recording,
    options,
  });
}

export async function stopReplay() {
  return invoke('stop_replay');
}
```

---

# 38. 必须支持的前端状态

```ts
type EngineState =
  | 'idle'
  | 'recording'
  | 'replaying'
  | 'stopping';
```

按钮状态必须与 EngineState 保持一致。

例如：

```text
recording:
    start recording -> disabled
    stop recording  -> enabled
    start replay    -> disabled

replaying:
    start recording -> disabled
    stop recording  -> disabled
    start replay    -> disabled
    stop replay     -> enabled
```

---

# 39. 数据流

完整数据流：

```text
用户真实鼠标
     │
     ▼
OS Input Event
     │
     ▼
rdev
     │
     ▼
RawMouseEvent
     │
     ▼
Recorder
     │
     ├── 记录时间
     ├── 处理 MouseMove 压缩
     └── 保存 Action
     │
     ▼
Recording
     │
     ├──── JSON 文件
     │
     └──── Frontend Timeline
```

回放：

```text
Recording
     │
     ▼
ReplayEngine
     │
     ├── delay
     ├── stop check
     └── execute
             │
             ▼
           enigo
             │
             ▼
        OS mouse input
             │
             ▼
          目标程序
```

---

# 40. 第一阶段实施顺序

不要一次把所有东西写完。

按照下面顺序实现。

## Phase 1：模型

实现：

```text
MouseButton
MouseAction
RecordedAction
Recording
EngineState
ReplayOptions
Error
```

完成 serde 序列化/反序列化测试。

---

## Phase 2：Recorder

实现：

```text
start()
stop()
MouseMove
ButtonDown
ButtonUp
Wheel
delay_ms
```

先不做复杂 MouseMove 压缩。

验证：

```text
用户移动鼠标
↓
可以正确得到 Action
```

---

## Phase 3：Replay

接入：

```text
enigo
```

实现：

```text
Move
ButtonDown
ButtonUp
Wheel
```

验证：

```text
录制
↓
停止
↓
立即回放
↓
用户观察结果
```

---

## Phase 4：循环

实现：

```text
loops
loop_interval_ms
speed
```

---

## Phase 5：Emergency Stop

实现：

```text
ESC
stop_flag
release_all_buttons()
```

---

## Phase 6：MouseMove 优化

加入：

```text
非拖拽状态压缩
拖拽状态保真
```

---

## Phase 7：前端 Timeline

实现：

```text
Action List
duration
position
button
wheel
```

---

## Phase 8：持久化

实现：

```text
save JSON
load JSON
schema version
```

---

# 41. MVP 验收标准

必须至少完成以下测试。

## 测试 1：普通点击

录制：

```text
Move -> Left Down -> Left Up
```

回放：

```text
目标位置正确点击
```

---

## 测试 2：右键

录制：

```text
Right Down -> Right Up
```

回放：

```text
正确触发右键菜单
```

---

## 测试 3：中键

验证：

```text
Middle Down
Middle Up
```

---

## 测试 4：拖拽文件

例如：

```text
文件 A
     ↓
拖动
     ↓
文件夹 B
```

必须能够真实完成拖拽。

---

## 测试 5：滚轮

验证：

```text
向上
向下
水平滚动（如果设备支持）
```

---

## 测试 6：长按

验证：

```text
ButtonDown
等待 1 秒
ButtonUp
```

必须仍然保持按住 1 秒。

---

## 测试 7：连续循环

例如：

```text
loops = 100
```

验证：

* 事件顺序正确
* 没有 Action 越来越多
* Recorder 不会记录 Replay 产生的事件
* 没有线程泄漏
* 没有按钮卡死

---

## 测试 8：紧急停止

在拖拽过程中：

```text
ESC
```

必须：

1. 停止 Replay
2. 释放所有已按下按钮
3. 状态恢复 Idle

---

## 测试 9：多显示器

验证：

```text
主屏
副屏
负坐标
```

都可以录制和回放。

---

# 42. 非功能要求

## 稳定性

不能因为：

```text
Recorder callback
Replay thread
Tauri event
```

任何一个异常导致整个 App 崩溃。

---

## 性能

录制普通鼠标行为时：

* CPU 占用应保持较低
* 不允许每个 MouseMove 都触发 React render
* 不允许每个 MouseMove 都执行磁盘写入

---

## 内存

Recording 存储在内存中即可，但需要避免无限制增长。

如果未来支持超长录制，需要进一步考虑：

```text
chunk
stream
disk-backed recording
```

第一版可以暂不实现。

---

# 43. 安全要求

这是一个会“控制用户鼠标”的应用，因此必须具备明确的停止机制。

必须满足：

```text
1. 用户明确点击开始回放之后才能自动操作
2. Replay 时 UI 明确显示当前状态
3. 提供明显的 Stop
4. 提供全局 Emergency Stop
5. 任何异常退出前尽量释放鼠标按钮
6. 不允许后台悄悄启动 Replay
7. 不允许未授权时自动运行
```

---

# 44. 暂时不要做的错误优化

不要一开始做：

```text
❌ AI
❌ OCR
❌ CV
❌ UI Automation
❌ Accessibility
❌ 虚拟鼠标
❌ 自动识别控件
❌ 自动重新定位
❌ 智能纠错
```

原因不是这些技术不可行。

而是第一阶段真正应该验证的是：

> **“用户录制一段鼠标操作，然后电脑能否稳定、准确地重复执行。”**

如果这件事情都没有验证，就没有必要提前建设复杂自动化能力。

---

# 45. 后续演进方向

当基础版本稳定之后，再考虑：

```text
                    Mouse Automation
                           │
          ┌────────────────┼────────────────┐
          │                │                │
       Keyboard          Window          Vision
          │                │                │
       Hotkey         Active Window        OCR
          │                │                │
          └────────────────┼────────────────┘
                           │
                    Accessibility
                           │
                    Semantic Action
```

最终可以逐渐从：

```text
纯坐标录制
```

升级成：

```text
行为录制
+
语义识别
+
视觉识别
+
条件控制
+
变量
+
工作流
```

形成真正的桌面自动化/RPA工具。

---

# 46. 给 Coding Agent 的执行要求

实现过程中必须遵守：

### 1. 不要修改产品范围

第一阶段只实现：

```text
Mouse Record
Mouse Replay
Loop
Stop
Persistence
UI
```

不要自行扩展 AI/OCR/Accessibility。

### 2. 不要直接依赖第三方 crate 类型作为领域模型

例如不要：

```rust
Vec<rdev::Event>
```

直接贯穿整个业务层。

必须转换成：

```rust
Recording
RecordedAction
MouseAction
```

### 3. 平台差异必须隔离

不要把大量：

```rust
#[cfg(target_os = "...")]
```

散落到业务代码中。

### 4. ReplayEngine 必须可测试

不要把所有逻辑写在 Tauri command 中。

应该：

```text
Tauri Command
    ↓
ReplayEngine
    ↓
MouseController
```

### 5. Recorder 必须可测试

推荐：

```text
EventSource
    ↓
Recorder
    ↓
Recording
```

可以让单元测试直接注入假的 `EventSource`。

### 6. 所有公共接口写清楚错误行为

尤其是：

```text
start_recording()
stop_recording()
start_replay()
stop_replay()
```

### 7. 不要为了“看起来完整”提前实现未来功能

优先保证 MVP 可以真实运行。

---

# 47. 推荐的最终核心抽象

最终希望得到类似：

```rust
pub struct AutomationEngine {
    recorder: Recorder,
    replayer: ReplayEngine,
    state: Arc<RwLock<EngineState>>,
}
```

Recorder：

```rust
pub struct Recorder {
    source: Box<dyn MouseEventSource>,
}
```

Replay：

```rust
pub struct ReplayEngine {
    controller: Box<dyn MouseController>,
}
```

核心使用方式：

```rust
engine.start_recording()?;

let recording = engine.stop_recording()?;

engine.start_replay(
    recording,
    ReplayOptions {
        loops: 100,
        loop_interval_ms: 500,
        speed: 1.0,
    },
)?;
```

---

# 48. 最终目标

第一阶段成功的标准非常简单：

> 用户坐在电脑前，点击“开始录制”，正常完成一段鼠标操作，点击“停止录制”，然后设置循环次数并点击“开始回放”，电脑能够尽可能按照原始的鼠标轨迹、按键状态、滚轮行为和时间节奏重复执行这段操作。

重点不是“智能”。

重点是：

```text
准确
稳定
可停止
可重复
```

在这个基础上，再逐步增加更高级的自动化能力。
