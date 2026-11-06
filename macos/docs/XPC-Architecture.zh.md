# Actrium XPC Multi-Process Architecture

> 历史文档。现行路径见 [Renderer-Process-Pool.md](Renderer-Process-Pool.md)。安装包不再包含 `RCTViewService*.xpc`。

## Core Principles

### 1:1:1 Architecture (重要原则)

```
Tab : ViewBridgeConnection : RCTRootView = 1 : 1 : 1
```

每个 Tab 独立拥有完整的渲染链路：
- 一个 `ViewBridgeHostView` (Host 端连接)
- 一个 `ActriumServiceViewController` (Service 端实例)
- 一个 `RCTRootView` (React Native 渲染视图)

**无论前台还是后台，每个 Tab 保持独立渲染状态。**

### 统一标识符

所有资源标识统一使用 **tabId** (`UUID`)：
- Host 端 Coordinator 按 tabId 管理
- Service 端 ViewController 按 tabId 区分
- IPC 消息按 tabId 路由

## Overview

Actrium 使用 ViewBridge XPC 架构将 React Native 渲染隔离到独立进程，实现：
- 进程隔离：RN 崩溃不影响主进程
- 资源复用：相同应用共享 RCTBridge（但各 Tab 独立 RCTRootView）
- 并行渲染：多个 Tab 可同时渲染

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                 Host App (Actrium)                                    │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                      │
│   ┌─────────────────────────────────────────────────────────────────────────────┐   │
│   │                          BrowserViewModel (tabs: [Tab])                      │   │
│   └─────────────────────────────────────────────────────────────────────────────┘   │
│          │                    │                    │                    │            │
│          ▼                    ▼                    ▼                    ▼            │
│   ┌────────────┐       ┌────────────┐       ┌────────────┐       ┌────────────┐     │
│   │   Tab A    │       │   Tab B    │       │   Tab C    │       │   Tab D    │     │
│   │ startPage  │       │  history   │       │   AppX     │       │   AppY     │     │
│   │fp:system.* │       │fp:system.* │       │fp:abc123   │       │fp:def456   │     │
│   └─────┬──────┘       └─────┬──────┘       └─────┬──────┘       └─────┬──────┘     │
│         │                    │                    │                    │            │
│         └────────────────────┼────────────────────┼────────────────────┘            │
│                              ▼                    ▼                                  │
│   ┌─────────────────────────────────────────────────────────────────────────────┐   │
│   │                         XPCServicePool (单例)                                 │   │
│   │  ┌─────────────────────────────────────────────────────────────────────────┐│   │
│   │  │  分配策略:                                                               ││   │
│   │  │  - system.* fingerprint → 固定 Slot 0                                   ││   │
│   │  │  - 相同 fingerprint → 共享 Slot                                          ││   │
│   │  │  - 不同 fingerprint → 分配新 Slot (1,2,3)                                ││   │
│   │  └─────────────────────────────────────────────────────────────────────────┘│   │
│   │                                                                              │   │
│   │  Slot 0: system.*     Slot 1: abc123    Slot 2: def456    Slot 3: (空闲)    │   │
│   │  refCount: 2          refCount: 1       refCount: 1       refCount: 0       │   │
│   │  (TabA, TabB)         (TabC)            (TabD)                              │   │
│   └─────────────────────────────────────────────────────────────────────────────┘   │
│                              │                    │                                  │
│                              ▼                    ▼                                  │
│   ┌─────────────────────────────────────────────────────────────────────────────┐   │
│   │                     ViewBridgeCoordinatorManager (单例)                       │   │
│   │  coordinators: [TabID: ViewBridgeCoordinator]                                │   │
│   │                                                                              │   │
│   │  TabA → Coordinator { allocatedServiceId: "...RCTViewService0", ... }        │   │
│   │  TabB → Coordinator { allocatedServiceId: "...RCTViewService0", ... }        │   │
│   │  TabC → Coordinator { allocatedServiceId: "...RCTViewService1", ... }        │   │
│   │  TabD → Coordinator { allocatedServiceId: "...RCTViewService2", ... }        │   │
│   └─────────────────────────────────────────────────────────────────────────────┘   │
│                              │                                                       │
│                              ▼                                                       │
│   ┌─────────────────────────────────────────────────────────────────────────────┐   │
│   │                    ViewBridgeHostView (每个 Tab 一个实例)                      │   │
│   │                                                                              │   │
│   │  TabA.HostView ─┐                                                            │   │
│   │  TabB.HostView ─┼─► NSRemoteViewController.requestViewController(...)        │   │
│   │  TabC.HostView ─┤     fromService: "io.actrium.rctviewservice{i}"   │   │
│   │  TabD.HostView ─┘                                                            │   │
│   └─────────────────────────────────────────────────────────────────────────────┘   │
│                              │                                                       │
└──────────────────────────────┼───────────────────────────────────────────────────────┘
                               │
                               │ ViewBridge (XPC + Mach IPC)
                               │ 每个 requestViewController 创建独立连接
                               ▼
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                              XPC Services (独立进程)                                 │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                      │
│  ┌───────────────────────────────────┐  ┌──────────────────┐  ┌──────────────────┐  │
│  │        RCTViewService0            │  │  RCTViewService1 │  │  RCTViewService2 │  │
│  │ io.actrium.rctviewservice0│  │ ...Service1      │  │ ...Service2      │  │
│  │                                   │  │                  │  │                  │  │
│  │  ┌─────────────────────────────┐  │  │ ┌──────────────┐ │  │ ┌──────────────┐ │  │
│  │  │  ServiceReactNativeHost     │  │  │ │    SRNH      │ │  │ │    SRNH      │ │  │
│  │  │  (进程单例, 共享 RCTBridge)   │  │  │ │  (单例)      │ │  │ │  (单例)      │ │  │
│  │  └─────────────────────────────┘  │  │ └──────────────┘ │  │ └──────────────┘ │  │
│  │                │                  │  │        │         │  │        │         │  │
│  │  ┌─────────────┴─────────────┐    │  │   ┌────┴────┐    │  │   ┌────┴────┐    │  │
│  │  │                           │    │  │   │         │    │  │   │         │    │  │
│  │  ▼                           ▼    │  │   ▼         │    │  │   ▼         │    │  │
│  │ ┌──────────────┐  ┌──────────────┐│  │┌──────────┐ │    │  │┌──────────┐ │    │  │
│  │ │ActriumService │  │ActriumService ││  ││ActriumSvc │ │    │  ││ActriumSvc │ │    │  │
│  │ │ViewController│  │ViewController││  ││VC (TabC) │ │    │  ││VC (TabD) │ │    │  │
│  │ │   (TabA)     │  │   (TabB)     ││  │└──────────┘ │    │  │└──────────┘ │    │  │
│  │ │              │  │              ││  │             │    │  │             │    │  │
│  │ │ ┌──────────┐ │  │ ┌──────────┐ ││  │┌──────────┐ │    │  │┌──────────┐ │    │  │
│  │ │ │RCTRootA  │ │  │ │RCTRootB  │ ││  ││RCTRootC  │ │    │  ││RCTRootD  │ │    │  │
│  │ │ │startPage │ │  │ │history   │ ││  ││  AppX    │ │    │  ││  AppY    │ │    │  │
│  │ │ └──────────┘ │  │ └──────────┘ ││  │└──────────┘ │    │  │└──────────┘ │    │  │
│  │ └──────────────┘  └──────────────┘│  └──────────────┘    │  └──────────────┘    │  │
│  └───────────────────────────────────┘                                             │  │
│                                                                                      │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

## Key Relationships

### 1:1:1 Mapping

| Host 端 | ViewBridge | Service 端 |
|---------|------------|------------|
| Tab (tabId: UUID) | ViewBridgeHostView | ActriumServiceViewController |
| ViewBridgeCoordinator | NSRemoteViewController | RCTRootView |

每个 Tab 的 `tabId` 贯穿整个链路，用于资源管理和消息路由。

### XPC Service 进程复用

同一个 XPC Service 进程内：
- 共享一个 `ServiceReactNativeHost` 单例（管理 RCTBridge）
- 共享一个 `RCTBridge` (JS Runtime)
- 每个 Tab 独立的 `ActriumServiceViewController` 实例
- 每个 Tab 独立的 `RCTRootView`（由各自的 ViewController 持有）

## Slot Allocation Strategy

| Fingerprint | Slot | 说明 |
|-------------|------|------|
| `system.*` | 0 | 系统应用固定使用 Slot 0 |
| 相同 fingerprint | 共享 | 同应用的不同 Tab 共享 Slot |
| 不同 fingerprint | 新分配 | 不同应用分配不同 Slot (1,2,3) |

## Bundle Identifiers

| Slot | Bundle Identifier |
|------|-------------------|
| 0 | `io.actrium.rctviewservice0` |
| 1 | `io.actrium.rctviewservice1` |
| 2 | `io.actrium.rctviewservice2` |
| 3 | `io.actrium.rctviewservice3` |

## IPC Communication

Transport: `DistributedNotificationCenter`
Protocol: Protobuf (定义在 `Shared/IPC/actrium_ipc.proto`)

### Messages

| Direction | Notification Name | Message Type | 说明 |
|-----------|-------------------|--------------|------|
| Host → Service | `io.actrium.tabConfig.<bundleId>` | `XPCTabConfig` | Tab 配置（包含 tabId） |
| Service → Host | `io.actrium.xpcServiceReady` | `XPCServiceReady` | Service 就绪通知 |
| Service → Host | `io.actrium.tabState.<tabId>` | `XPCTabState` | Tab 渲染状态 |
| Service → Host | `io.actrium.tabNavigation.<tabId>` | `XPCTabNavigation` | 导航请求 |

### Key: tabId (UUID)

所有消息均包含 `tab_id` 字段，用于：
1. Service 内部路由到正确的 ActriumServiceViewController
2. Host 端匹配到正确的 ViewBridgeCoordinator
3. 资源生命周期管理

## Files

| File | 说明 |
|------|------|
| `Shared/IPC/actrium_ipc.proto` | Protobuf 定义（含 XPC 消息） |
| `Actrium/MultiProcess/XPCServicePool.swift` | 进程池管理 |
| `Actrium/MultiProcess/ViewBridgeHostView.swift` | Host 端 ViewBridge 封装 |
| `ActriumViewService/ActriumServiceViewController.swift` | Service 端 ViewController |
| `scripts/generate_xpc_pool.sh` | Build Phase 脚本，生成 4 个 XPC service |
| `docs/XPC-Architecture.md` | 本文档 |
