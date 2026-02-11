## 系统架构概览

AskAway-iPad 是一个类浏览器的 iPad 应用，采用 Native iOS 外壳 + React Native 运行时的混合架构。核心能力是在沙箱化的标签页中加载 AI 驱动的"actr"插件（guest app），通过 WebRTC 与后端 AI 服务通信，支持文本对话、语音流式传输、附件上传等。

### 顶层目录

| 目录 | 职责 |
|---|---|
| `Canvas/` | Native iOS 应用壳（Swift/ObjC），提供标签页管理、原生模块 |
| `Subsystem/` | React Native 运行时，所有 JS/TS 业务逻辑所在 |
| `openspec/` | Spec 驱动开发体系（提案、规范、项目约定） |
| `Scripts/` | CI/CD 与构建脚本 |
| `Pods/` | CocoaPods 原生依赖 |

### Subsystem/src 模块划分

```
Subsystem/src/
├── TabRouter.tsx          # 顶层路由：根据 tab 类型分发到 actr 插件或系统应用
├── api/                   # HTTP API 层（应用信息、认证、反馈）
├── actr-host/             # 核心宿主系统 ──────────────────────────
│   ├── ActrApp.tsx        #   插件标签页总调度（Engine 池管理、DevTools）
│   ├── ActrTabView.tsx    #   单标签页生命周期（Engine 初始化、布局）
│   ├── views/             #   页面级视图（Chat、LoadApp、ChatEmpty）
│   ├── components/        #   UI 组件（输入框、气泡、附件、语音覆盖层等）
│   ├── hooks/             #   宿主专用 hooks（录音、播放、附件、工具栏状态）
│   ├── store/             #   Zustand 状态管理（消息、输入、应用、标签映射等）
│   ├── contexts/          #   React Context（TabContext）
│   ├── libs/              #   宿主内部库（Guest 事件分发、HTTP 代理、TTS、DevTools）
│   ├── utils/             #   工具函数（.actr 加载器、音频管理器、面板提取）
│   └── types/             #   类型定义
├── native-bridge/         # 原生桥接层（WebRTC 连接、配置生成）
├── hooks/                 # 全局 hooks（连接管理、认证、登录流程、计时器）
├── repository/            # SQLite 持久化层（消息、应用、书签、认证令牌）
├── services/              # 业务服务（会话初始化、附件选取、图片上传）
├── libs/                  # 基础设施（SQLite 单例、Schema 初始化）
├── system-apps/           # 系统应用（登录、历史记录、书签）
├── i18n/                  # 国际化（8 种语言）
└── utils/                 # 通用工具（UUID、Base64、SHA-256）
```

---

## 模块职责与协作关系

### 分层架构

```
┌─────────────────────────────────────────────────┐
│  Native iOS (Canvas/)                           │
│  标签页管理、原生模块（Actr SDK、Speech、Camera）  │
└──────────────────────┬──────────────────────────┘
                       │ NativeModules / NativeEventEmitter
┌──────────────────────▼──────────────────────────┐
│  native-bridge/                                 │
│  ActrConnection: WebRTC 连接抽象（引用计数、流路由）│
│  ActrConfigUtils: TOML 配置生成                  │
└──────────────────────┬──────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────┐
│  hooks/ (全局)                                   │
│  useActrConnection: 连接生命周期管理              │
│  useAuth / useLoginFlow: 认证状态机              │
└──────────────────────┬──────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────┐
│  actr-host/ (核心宿主)                           │
│  ActrApp → ActrTabView → Chat → ActrInput       │
│  状态管理(store/) + 业务 hooks + UI 组件          │
└──────────────────────┬──────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────┐
│  services/ + repository/ + api/                 │
│  会话初始化、数据持久化、远程 API 调用             │
└─────────────────────────────────────────────────┘
```

### 关键协作

- **TabRouter** 接收 Native 传入的标签页配置，按类型路由到 `ActrApp`（插件）或系统应用（登录/历史/书签）
- **ActrApp** 管理 Rill Engine 实例池，每个标签页对应一个沙箱化的 guest 运行时
- **ActrTabView** 负责单个标签页的 Engine 生命周期：下载 `.actr` 包 → 校验 → 加载 guest 脚本
- **Chat** 是核心交互界面，通过 `ActrInput` 整合文本/语音/附件输入
- **native-bridge** 是 JS 与原生 Actr SDK 之间的唯一通道，通过 `ActrStreamRouter`（单例）将原生事件路由到正确的连接实例
- **services/AppSessionService** 是会话初始化的编排者：API 获取应用信息 → 持久化 → 下载包 → 校验完整性 → 解析脚本

### 为什么这样划分

- **native-bridge 独立**：隔离原生通信复杂性（引用计数、流路由、热重载安全），上层无需关心 WebRTC 细节
- **hooks 分两层**：全局 hooks（连接、认证）可被任何模块使用；actr-host/hooks 是宿主专用（录音、附件），避免职责泄漏
- **store 按领域拆分**：消息、输入、应用、标签映射各自独立，避免单一巨型 store
- **repository 与 store 分离**：store 管理内存状态 + 触发异步持久化，repository 只负责 SQLite 读写，职责清晰

---

## 核心数据流

### 1. 文本消息发送与接收

```
用户输入文本 → useInputStore 暂存
  → ActrInput.processMessageSend()
    → 创建 IMessage 写入 useMessageStore（同步 UI）+ MessageRepository（持久化）
    → useActrConnection.sendMessage()
      → ActrConnection.sendMessage() → Native Actr SDK → WebRTC → 后端

后端响应 → Native ActrStreamMessage 事件
  → ActrStreamRouter 按 streamId 路由
    → ActrConnection 的 chat listener
      → useActrConnection 的 onMessage 回调
        → useMessageStore.updateMessage()（累加内容，流式更新 UI）
```

### 2. 语音流式传输

```
用户按住语音按钮 → handleVoiceStart()
  → startRecording()（麦克风开始采集 PCM 16kHz/16bit/mono）
  → 切片每 ~128ms 产生一个，push 到 chunkQueue（缓冲阶段）

1s 阈值到达 → onThresholdReached()
  → processMessageSend() 创建消息
  → startVoice() 通知服务端开始语音会话
  → 开闸：drainQueue() flush 积压切片
  → 之后新切片到来时自动 drain（等效实时模式）

用户松手 → handleVoiceEnd()
  → stopRecording()
  → 等待队列 drain 完毕
  → endVoice() 通知服务端语音结束

误触 (<1s) → 停止录音 + 清空队列，零服务端交互
```

### 3. 应用会话初始化

```
用户扫码/输入 URL → LoadApp 提取 appId → 写入 useTabIdMapStore
  → Chat 挂载 → AppSessionService.initSession(appId)
    → API 获取 AppInfo → 写入 useAppStore + AppRepository
    → ActrLoader 下载 .actr 包 → 解压 → SHA-256 校验 → 解析 JS 脚本
    → 脚本写入 useAppStore.guestScripts
  → ActrTabView 检测到 guestScript → Engine.loadBundle() → Guest 运行

Guest 与 Host 通信 → Engine message 事件
  → eventHandler 分发（HTTP_REQUEST → httpHandler, SPEECH_REQUEST → hostToNative）
```

### 4. 认证流程

```
操作触发权限检查 → useAuth.hasAuth()
  → 未登录：打开系统登录标签页（NativeModules.TabModule.openTab）
  → LoginApp: 微信/Apple OAuth → Native 回调 → useLoginFlow 状态机
    → checkSession → loginOptions → authorizing → success
    → 写入 useUserStore + AuthRepository
    → 切回原标签页

已登录未授权 → authApi.authorize() → 写入 useAuthTaskStore.alreadyAuthorizedAppIds
```

### 边界点标注

| 边界 | 位置 | 说明 |
|---|---|---|
| JS ↔ Native | native-bridge/ActrConnection | NativeModules + NativeEventEmitter |
| 客户端 ↔ 后端 | ActrConnection.sendMessage/startVoice | WebRTC 数据通道 |
| 客户端 ↔ API | api/https | HTTP REST（应用信息、认证、反馈） |
| 内存 ↔ 持久化 | store → repository | Zustand 状态变更触发异步 SQLite 写入 |
| Host ↔ Guest | actr-host/libs/eventHandler | Rill Engine message 事件双向通信 |
| 客户端 ↔ COS | services/ImageUploadService | 腾讯云对象存储（附件上传） |
