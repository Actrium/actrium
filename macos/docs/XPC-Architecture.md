# Actrium XPC Multi-Process Architecture

> History document. See current path [Renderer-Process-Pool.md](Renderer-Process-Pool.md). Install package no longer contains `RCTViewService*.xpc`.

## Core Principles

### 1:1:1 Architecture (Key principles)

```
Tab : ViewBridgeConnection : RCTRootView = 1 : 1 : 1
```

Every one. Tab The following is a full-fledged repertoire:
- One. `ViewBridgeHostView` (Host End Connection)
- One. `ActriumServiceViewController` (Service Side instance)
- One. `RCTRootView` (React Native Render View)

**For the front and the back, every one. Tab Keeps the status of independent rendering.**

### Uniform identifier

Harmonization of all resource markers **tabId** (`UUID`):
- Host End Coordinator Press tabId Management
- Service End ViewController Press tabId Distinction
- IPC Message Press tabId Route

## Overview

Actrium Use ViewBridge XPC Structure will React Native Rendering isolation to the independence process, achieving:
- Process isolation:RN The crash does not affect the main process
- Reuse of resources: share of the same application RCTBridge(but each) Tab Independence RCTRootView)
- Parallel rendering: multiple Tab But it's replicating.

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
│   │                         XPCServicePool (Single cases)                                 │   │
│   │  ┌─────────────────────────────────────────────────────────────────────────┐│   │
│   │  │  Distribution Policy:                                                               ││   │
│   │  │  - system.* fingerprint → Fixed Slot 0                                   ││   │
│   │  │  - Same fingerprint → Share Slot                                          ││   │
│   │  │  - Different. fingerprint → Assign New Slot (1,2,3)                                ││   │
│   │  └─────────────────────────────────────────────────────────────────────────┘│   │
│   │                                                                              │   │
│   │  Slot 0: system.*     Slot 1: abc123    Slot 2: def456    Slot 3: (Free)    │   │
│   │  refCount: 2          refCount: 1       refCount: 1       refCount: 0       │   │
│   │  (TabA, TabB)         (TabC)            (TabD)                              │   │
│   └─────────────────────────────────────────────────────────────────────────────┘   │
│                              │                    │                                  │
│                              ▼                    ▼                                  │
│   ┌─────────────────────────────────────────────────────────────────────────────┐   │
│   │                     ViewBridgeCoordinatorManager (Single cases)                       │   │
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
│   │                    ViewBridgeHostView (Every one. Tab An example.)                      │   │
│   │                                                                              │   │
│   │  TabA.HostView ─┐                                                            │   │
│   │  TabB.HostView ─┼─► NSRemoteViewController.requestViewController(...)        │   │
│   │  TabC.HostView ─┤     fromService: "io.actrium.actro.rctviewservice{i}"   │   │
│   │  TabD.HostView ─┘                                                            │   │
│   └─────────────────────────────────────────────────────────────────────────────┘   │
│                              │                                                       │
└──────────────────────────────┼───────────────────────────────────────────────────────┘
                               │
                               │ ViewBridge (XPC + Mach IPC)
                               │ Every one. requestViewController Create independent connection
                               ▼
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                              XPC Services (Independence process)                                 │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                      │
│  ┌───────────────────────────────────┐  ┌──────────────────┐  ┌──────────────────┐  │
│  │        RCTViewService0            │  │  RCTViewService1 │  │  RCTViewService2 │  │
│  │ io.actrium.actro.rctviewservice0│  │ ...Service1      │  │ ...Service2      │  │
│  │                                   │  │                  │  │                  │  │
│  │  ┌─────────────────────────────┐  │  │ ┌──────────────┐ │  │ ┌──────────────┐ │  │
│  │  │  ServiceReactNativeHost     │  │  │ │    SRNH      │ │  │ │    SRNH      │ │  │
│  │  │  (Process Single, Share RCTBridge)   │  │  │ │  (Single cases)      │ │  │ │  (Single cases)      │ │  │
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

| Host End | ViewBridge | Service End |
|---------|------------|------------|
| Tab (tabId: UUID) | ViewBridgeHostView | ActriumServiceViewController |
| ViewBridgeCoordinator | NSRemoteViewController | RCTRootView |

Every one. Tab It's... `tabId` The whole link is used for resource management and information routing.

### XPC Service Process reuse

The same. XPC Service Within the process:
- Share one `ServiceReactNativeHost` Single cases (management) RCTBridge)
- Share one `RCTBridge` (JS Runtime)
- Every one. Tab Independent `ActriumServiceViewController` Examples
- Every one. Tab Independent `RCTRootView`(by each of the ViewController held)

## Slot Allocation Strategy

| Fingerprint | Slot | Annotations |
|-------------|------|------|
| `system.*` | 0 | Fixed use of the system Slot 0 |
| Same fingerprint | Share | Different from application Tab Share Slot |
| Different. fingerprint | New distribution | Different applications are distributed differently Slot (1,2,3) |

## Bundle Identifiers

| Slot | Bundle Identifier |
|------|-------------------|
| 0 | `io.actrium.actro.rctviewservice0` |
| 1 | `io.actrium.actro.rctviewservice1` |
| 2 | `io.actrium.actro.rctviewservice2` |
| 3 | `io.actrium.actro.rctviewservice3` |

## IPC Communication

Transport: `DistributedNotificationCenter`
Protocol: Protobuf (Define in `Shared/IPC/actrium_ipc.proto`)

### Messages

| Direction | Notification Name | Message Type | Annotations |
|-----------|-------------------|--------------|------|
| Host → Service | `com.actro.tabConfig.<bundleId>` | `XPCTabConfig` | Tab Configure (includes) tabId) |
| Service → Host | `com.actro.xpcServiceReady` | `XPCServiceReady` | Service Ready |
| Service → Host | `com.actro.tabState.<tabId>` | `XPCTabState` | Tab Render Status |
| Service → Host | `com.actro.tabNavigation.<tabId>` | `XPCTabNavigation` | Navigation Request |

### Key: tabId (UUID)

All messages are included. `tab_id` fields for:
1. Service Internal route is right. ActriumServiceViewController
2. Host The end matches right. ViewBridgeCoordinator
3. Resource life-cycle management

## Files

| File | Annotations |
|------|------|
| `Shared/IPC/actrium_ipc.proto` | Protobuf Definitions XPC Message) |
| `Actrium/MultiProcess/XPCServicePool.swift` | Process pool management |
| `Actrium/MultiProcess/ViewBridgeHostView.swift` | Host End ViewBridge Encapsulate |
| `ActriumViewService/ActriumServiceViewController.swift` | Service End ViewController |
| `scripts/generate_xpc_pool.sh` | Build Phase Script, Generate 4 individual XPC service |
| `docs/XPC-Architecture.md` | This document |
