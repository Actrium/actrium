# Loom (Subsystem) 业务内核架构

`Subsystem` 是项目的跨平台业务逻辑中心。作为独立仓库存在，它不依赖特定的 Native UI，而是通过标准的 Bridge 与宿主壳通信。

## 1. 核心模块划分

```
Subsystem/src/
├── TabRouter.tsx          # 顶级路由：分发至 ActrApp (插件) 或 SystemApp (内置功能)
├── actr-host/             # 核心宿主逻辑
│   ├── ActrApp.tsx        #   多 Tab 管理与全局监控推送
│   ├── ActrTabView.tsx    #   插件沙箱 (Rill Engine) 的完整生命周期管理
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
- **Pad (>=768px)**: 启用 `ActrInputPadView`，采用固定宽度的侧边工具栏和三栏布局。
- **Phone**: 启用 `ActrInputMobileView`，采用全宽输入框、大按钮手势操作区和底部 Overlay。

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
