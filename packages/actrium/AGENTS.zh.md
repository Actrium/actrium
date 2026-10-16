# Actrium Runtime (ActriumRuntime) 业务内核架构

`ActriumRuntime` 是项目的跨平台业务逻辑中心。作为独立仓库存在，它不依赖特定的 Native UI，而是通过标准的 Bridge 与宿主壳通信。

## 1. 核心模块划分

```
ActriumRuntime/src/
├── TabRouter.tsx          # 顶级路由：分发至 RuntimeApp (插件) 或 SystemApp (内置功能)
├── runtime-host/             # 核心宿主逻辑
│   ├── RuntimeApp.tsx        #   多 Tab 管理与全局监控推送
│   ├── RuntimeTabView.tsx    #   插件沙箱 (Keel Engine) 的完整生命周期管理
│   ├── components/        #   支持响应式的双端 UI 组件库
│   └── store/             #   Zustand 状态机 (Message, App, Input 等领域)
├── native-bridge/         # 原生通信层：封装了 Actr WebRTC 的复杂信号逻辑
├── repository/            # 数据层：基于 SQLite 的本地持久化 (MessageRepository)
├── services/              # 编排服务：App 会话初始化、附件上传、资源下载
└── i18n/                  # 国际化支持
```

---

## 2. 关键设计模式

### A. 双端响应式适配 (Layout Tokens)
通过 `src/utils/layout.ts` 定义一套视觉 Token。系统监听 `window` 尺寸变化：
- **Pad (>=768px)**: 启用 `AppInputPadView`，采用固定宽度的侧边工具栏和三栏布局。
- **Phone**: 启用 `AppInputMobileView`，采用全宽输入框、大按钮手势操作区和底部 Overlay。

### B. 插件沙箱 (Host-Guest)
- 每个插件运行在隔离的 `Engine` 实例中。
- `ActrHost` 作为控制面，通过 `eventHandler` 监听 Guest 触发的请求（如 HTTP 代理、TTS 播放、语音采集通知）。

---

## 3. 核心数据流

### AI 对话流 (Flow)
1.  **Input**: 用户触发 `processMessageSend`。
2.  **Persistence**: 消息先同步计入本地 SQLite (`MessageRepository`) 并更新 Zustand Store。
3.  **Transport**: 调用 `native-bridge` 中的通道，将数据推向原生 Actr SDK。
4.  **Reaction**: 原生异步回传消息片段，通过 `ActrStreamRouter` 返回给对应的业务监听器，实现 UI 消息的流式累加。

### 语音处理流 (Audio)
- **Pad**: 点击触发录音状态，通过宿主 UI 实时呈现波形。
- **Phone**: 长按输入区触发，使用 `VoiceOverlay` 覆盖全局，支持“上划取消”等手势。
- **发送**: 持续切片 (Slices) 缓存 -> 触发阈值后开启实时 WebRTC 传输 -> 松手结束。

---

## 4. 协作守则
- **UI 处理**: 避免在 UI 层写复杂的业务判定，应尽量下沉至 `hooks` 或 `services`。
- **数据一致性**: 内存 Store 必须与本地 SQLite 保持最终一致，原则上由 Store 触发 Repository 写入。

## 5. iOS / Android 安全区处理约定

### 顶部安全区（Header）
- 顶部安全区不是全局单点处理，而是按子域分开：
  - `runtime-host`（插件会话域）：由 `runtime-host/components/ChatHeader.tsx` 负责可配置的顶部 inset。
  - `system-apps`（系统应用域）：各页面/组件自行控制顶部留白（当前多数为固定 `paddingTop`，例如 `SettingsMain.tsx` / `GlobalSearchApp.tsx` 的 `paddingTop: 60`）。
- `ChatHeader` 的具体规则（仅适用于 `runtime-host`）：
  - 使用 `useSafeAreaInsets().top` 读取系统顶部 inset。
  - `includeSafeTopInset=true`（默认）时，Header 高度为 `CHAT_HEADER_HEIGHT + topInset`，并通过 `paddingTop` 吃掉刘海/状态栏区域。
  - 当外层已处理顶部偏移时关闭内部 inset，避免重复叠加（如 `RuntimeTabView.tsx` 外层 `marginTop: topInsetRaw`，对应 `includeSafeTopInset={false}`）。

### 底部安全区（Input/Dock）
- 统一通过 `runtime-host/components/SafeBottomView.tsx` 处理底部安全区，不在业务页面重复写平台分支。
- `SafeBottomView` 规则：
  - iOS：`extraBottom = 0`，不额外叠加 `insets.bottom`（由 `SafeAreaView` 自身处理底部安全区）。
  - Android：`extraBottom = max(insets.bottom, minBottom)`，保证手势导航条机型和普通机型都有足够底部留白。
  - 最终 `paddingBottom = 原有 paddingBottom + extraBottom`，不会覆盖调用方已有样式。
- 典型用法：
  - `AppInputMobileView.tsx`：`<SafeBottomView minBottom={10} />`
  - `LoadApp.tsx`（Phone 输入 Dock）：`<SafeBottomView minBottom={12} />`

### 实施原则
- 新增底部输入栏、浮层 Dock、悬浮操作条时，优先复用 `SafeBottomView`，不要在页面内直接写 `Platform.OS` + `insets.bottom`。
- `runtime-host` 新增顶部 Header/导航条时，先判断外层是否已做 `top` 偏移，再决定 `includeSafeTopInset`，避免双重安全区。
- `system-apps` 新增页面时，不要默认复用 `ChatHeader` 语义；需在页面内明确选择“固定顶部留白”或“基于 `useSafeAreaInsets().top` 动态留白”，并保持同类系统页一致。


### 关于键盘


| 平台 | Native 处理 | RN 处理 |
| :--- | :--- | :--- |
| iOS Phone | 无 | KeyboardStickyView 吸附输入框 + useReanimatedKeyboardAnimation 实时压缩内容区（跟手动画） |
| iOS Pad | 无 | keyboardInset 静态偏移（事件驱动，键盘弹完后跳变） |
| Android Phone | 宿主 Native windowSoftInputMode 推起整个视图 | 不叠加额外偏移 |
| Android Pad | 宿主 Native windowSoftInputMode 推起整个视图 | 完全零干预 |


### 重要

1. 所有改动要考虑 iOS 和 Android 的兼容
2. 深色浅色模式兼容方式

```js
  const makeStyles = (colors: ColorTokens) => StyleSheet.create({ ... })
  const styles = useMemo(() => makeStyles(colors), [colors])
```