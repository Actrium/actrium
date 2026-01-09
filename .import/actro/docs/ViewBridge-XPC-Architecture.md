# ViewBridge XPC 架构文档

## 1. 架构概述

Canvas 使用 ViewBridge (macOS 私有框架) 实现跨进程视图渲染，将 React Native 内容运行在独立的 XPC Service 进程中。

```
┌─────────────────────────────────────────────────────────────────┐
│                        Canvas Host App                          │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐ │
│  │   SwiftUI View  │  │  MetroManager   │  │ XPCServicePool  │ │
│  │   (TabPanel)    │  │  (Port: 动态)   │  │  (4 slots)      │ │
│  └────────┬────────┘  └────────┬────────┘  └────────┬────────┘ │
│           │                    │                    │           │
│  ┌────────▼────────┐           │                    │           │
│  │ ViewBridgeHost  │           │                    │           │
│  │     View        │◄──────────┼────────────────────┘           │
│  │ (NSRemoteView)  │           │                                │
│  └────────┬────────┘           │                                │
└───────────┼────────────────────┼────────────────────────────────┘
            │                    │
            │ ViewBridge         │ DistributedNotificationCenter
            │ (macOS Private)    │
            │                    │
┌───────────▼────────────────────▼────────────────────────────────┐
│                    XPC Service Process                          │
│              (RCTViewService0/1/2/3.xpc)                        │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐ │
│  │ CanvasService   │  │ ServiceReact    │  │   RCTBridge     │ │
│  │ ViewController  │  │ NativeHost      │  │  (共享实例)      │ │
│  └────────┬────────┘  └────────┬────────┘  └────────┬────────┘ │
│           │                    │                    │           │
│  ┌────────▼────────────────────▼────────────────────▼────────┐ │
│  │              RCTRootView (per Tab)                        │ │
│  │         渲染尺寸由 containerSize 决定                       │ │
│  └───────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
            │
            │ WebSocket (Metro Protocol)
            │
┌───────────▼─────────────────────────────────────────────────────┐
│                     Metro Bundler                               │
│              (动态端口, 如 50319, 58242, 65045)                  │
│  - 打包 JS Bundle                                               │
│  - Hot Reload 支持                                              │
│  - 源码映射                                                      │
└─────────────────────────────────────────────────────────────────┘
```

## 2. 关键组件

### 2.1 Host 端 (Canvas App)

| 组件 | 文件 | 职责 |
|------|------|------|
| `ViewBridgeHostView` | ViewBridgeHostView.swift | NSRemoteViewController 包装，管理跨进程视图 |
| `ViewBridgeCoordinator` | ViewBridgeHostView.swift | 管理单个 Tab 的 XPC 连接生命周期 |
| `ViewBridgeMultiProcessTabView` | ViewBridgeHostView.swift | SwiftUI 视图，集成 ViewBridge |
| `XPCServicePool` | XPCServicePool.swift | 管理 4 个 XPC Service slot 的分配 |
| `MetroManager` | MetroManager.swift | Metro bundler 生命周期管理 |
| `TabPanel` | WorkspaceMainView.swift | Tab 卡片视图，管理渲染尺寸 |

### 2.2 XPC Service 端

| 组件 | 文件 | 职责 |
|------|------|------|
| `CanvasServiceViewController` | CanvasServiceViewController.swift | 动态注册的 NSServiceViewController |
| `ServiceReactNativeHost` | CanvasServiceViewController.swift | 管理共享 RCTBridge 和 per-Tab RCTRootView |
| `TabViewControllerEntry` | CanvasServiceViewController.swift | 单个 Tab 的 ViewController 和 RCTRootView |
| `ResizableContainerView` | CanvasServiceViewController.swift | 可调整尺寸的容器，响应 ViewBridge 尺寸同步 |
| `MetroConfig` | CanvasServiceViewController.swift | 存储从 Host 接收的 Metro 配置 |

## 3. 尺寸计算 (关键数据)

### 3.1 坐标系统

基于日志分析的实际数值：

| 参数 | 典型值 | 说明 |
|------|--------|------|
| `viewportSize` | 589 x 398 | 实际视口尺寸（窗口内容区域） |
| `cardWidth` | 186.7 | Overview 模式下的卡片宽度 |
| `cardHeight` | 126.2 | Overview 模式下的卡片高度 |
| `scale` | 3.154 | Focus 模式的缩放比例 (viewportSize / cardSize) |

### 3.2 尺寸关系

```
viewportSize.width / cardWidth = scale
589 / 186.7 ≈ 3.154

viewportSize.height / cardHeight = scale
398 / 126.2 ≈ 3.154
```

### 3.3 渲染模式

| 模式 | ViewBridge 渲染尺寸 | Canvas Transform | 最终显示尺寸 |
|------|---------------------|------------------|--------------|
| Overview | cardSize (187x126) | scale=1 | 187x126 |
| Focus | viewportSize (589x398) | scale=3.154 | 589x398 |

**关键发现**：ViewBridge XPC 在独立进程渲染，不受 Host 端 Canvas Transform 影响。Focus 模式必须让 XPC Service 用 viewportSize 渲染，否则内容会模糊。

## 4. 通信机制

### 4.1 DistributedNotificationCenter 通知

| 通知名称 | 方向 | 用途 |
|----------|------|------|
| `com.canvas.xpcServiceReady` | XPC → Host | XPC Service 就绪，可接收配置 |
| `com.canvas.tabConfig.<bundleId>` | Host → XPC | 发送 Tab 配置（tabId, systemAppType 等） |
| `com.canvas.metro.config` | Host → XPC | 广播 Metro 配置（host, port） |
| `com.canvas.containerSizeChanged` | Host → XPC | 容器尺寸变化通知 |
| `com.canvas.openBookmark` | XPC → Host | 用户点击书签 |
| `com.canvas.openHistoryItem` | XPC → Host | 用户点击历史记录 |

### 4.2 Tab Config 结构

```json
{
  "tabId": "90009605-812D-4507-BDFA-126296F9B672",
  "title": "Start Page",
  "systemAppType": "startPage",
  "backgroundColor": "#260f43",
  "containerWidth": 186.74,
  "containerHeight": 126.19
}
```

### 4.3 Metro Config 结构

```json
{
  "host": "127.0.0.1",
  "port": 65045
}
```

## 5. 启动时序 (关键)

```
┌─────────────────────────────────────────────────────────────────┐
│ 1. Canvas App 启动                                              │
│    - CrashHandler.setup()                                       │
│    - TabProcessManager 初始化                                    │
│    - XPCNotificationBridge 初始化                                │
│    - [DEBUG] MetroManager.start() 自动启动                       │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│ 2. Metro Bundler 启动 (异步)                                     │
│    - 动态分配端口 (findAvailablePort)                            │
│    - 等待 "Dev server ready" 输出                                │
│    - broadcastMetroConfig() 广播配置                             │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│ 3. Tab 创建 & XPC 分配                                          │
│    - XPCServicePool.allocate() 分配 service slot                │
│    - ViewBridgeHostView 创建，请求 NSRemoteViewController        │
│    - setupXPCReadyListener() 监听 xpcServiceReady               │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│ 4. XPC Service 启动                                             │
│    - registerCanvasServiceViewController() 动态注册类            │
│    - ServiceReactNativeHost.shared 初始化                        │
│    - setupTabConfigListener() 设置通知监听                       │
│    - postXPCServiceReady() 通知 Host                            │
│    ⚠️ 此时还未创建 RCTBridge (等待 Metro config)                 │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│ 5. Host 收到 xpcServiceReady                                    │
│    - 广播 Metro config 到 XPC Service                           │
│    - 检查 hasConnection && xpcServiceReady                      │
│    - sendTabConfigToService() 发送 Tab 配置                     │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│ 6. XPC Service 收到 Metro config                                │
│    - MetroConfig.update() 更新配置                              │
│    - ensureBridgeCreated() 创建 RCTBridge                       │
│    ✅ RCTBridge 使用正确的 Metro 端口                            │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│ 7. XPC Service 收到 Tab config                                  │
│    - handleTabConfigNotification()                              │
│    - createRootViewWithConfig() 创建 RCTRootView                │
│    - RN 开始渲染内容                                             │
└─────────────────────────────────────────────────────────────────┘
```

**关键发现**：RCTBridge 必须在收到 Metro config 之后才能创建，否则会使用默认的 8081 端口导致连接失败。

## 6. XPC Service Pool

### 6.1 配置

- **Pool 大小**: 4 个 slot (RCTViewService0 - RCTViewService3)
- **Bundle ID 格式**: `chat.askaway.Canvas.RCTViewService{N}`

### 6.2 分配策略

```swift
// 相同 fingerprint 的 Tab 共享同一个 XPC Service
// 不同 fingerprint 分配不同的 XPC Service
// 系统应用统一使用 slot 0
```

| 应用类型 | Fingerprint | 分配的 Service |
|----------|-------------|----------------|
| Start Page | system.startPage | RCTViewService0 |
| Bookmarks | system.bookmarks | RCTViewService0 |
| History | system.history | RCTViewService0 |
| 用户应用 A | {appFingerprint} | RCTViewService1 |
| 用户应用 B | {appFingerprint} | RCTViewService2 |

## 7. 已解决的问题

### 7.1 Metro 端口不匹配

**问题**: RCTBridge 创建时使用默认 8081 端口，但 Metro 运行在动态端口

**解决**: 延迟创建 RCTBridge，等待收到 Metro config 通知后再创建

### 7.2 Focus 模式渲染模糊

**问题**: ViewBridge XPC 用 cardSize (187x126) 渲染，Canvas Transform 放大后模糊

**解决**: Focus 模式下让 XPC Service 用 viewportSize (589x398) 渲染，然后用 scaleEffect 缩小，最后 Canvas Transform 放大

### 7.3 尺寸同步不可靠

**问题**: ViewBridge 私有 API 尺寸同步不稳定

**解决**: 添加 DistributedNotificationCenter 备用机制 (`com.canvas.containerSizeChanged`)

## 8. 文件清单

```
Canvas/
├── Canvas/
│   ├── App/
│   │   └── CanvasApp.swift              # 应用入口，Metro 自动启动
│   ├── MultiProcess/
│   │   ├── ViewBridgeHostView.swift     # ViewBridge Host 端实现
│   │   ├── XPCServicePool.swift         # XPC Service 池管理
│   │   └── TabProcessManager.swift      # Tab 进程管理
│   ├── DevTools/
│   │   ├── MetroManager.swift           # Metro bundler 管理
│   │   └── MetroAddressResolver.swift   # Metro 地址解析
│   └── Views/
│       └── WorkspaceMainView.swift      # TabPanel, 渲染尺寸计算
├── CanvasViewService/
│   └── CanvasServiceViewController.swift # XPC Service 实现
├── scripts/
│   └── generate_xpc_pool.sh             # XPC Service Pool 生成脚本
└── RCTNApps/
    └── src/
        ├── TabRouter.tsx                # RN Tab 路由
        └── system-apps/
            ├── StartPageApp.tsx
            ├── BookmarksApp.tsx
            └── HistoryApp.tsx
```

## 9. 调试技巧

### 9.1 日志过滤

```bash
# Host 端日志
log show --predicate 'process == "Canvas"' --last 1m

# XPC Service 日志
log show --predicate 'process contains "CanvasViewService"' --last 1m

# Metro 相关
log show --predicate 'eventMessage contains "Metro"' --last 1m
```

### 9.2 关键日志标记

| 标记 | 含义 |
|------|------|
| `📐` | 尺寸相关 |
| `📤` | 发送通知/配置 |
| `📡` | 接收通知/建立连接 |
| `🔵` | ViewBridge 相关 |
| `📦` | XPC Service Pool |
| `🆕` | 创建新实例 |

---

*文档生成时间: 2026-01-09*
*基于 Canvas 项目实际运行日志分析*
