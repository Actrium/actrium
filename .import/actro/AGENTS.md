# Teal — AskAway Android 客户端

## 项目概述

AskAway 是一款 AI 对话应用。Teal 仓库是其 Android 客户端，采用 **Kotlin 原生壳 + React Native 业务层** 的混合架构。原生壳负责多 Tab 窗口管理和系统级能力，RN 层（代号 Loom）承载全部业务 UI 和逻辑。

## 仓库结构

```
Teal/
├── android/                  # Android 原生壳 (Kotlin)
│   ├── app/                  #   主应用模块
│   │   └── src/main/java/com/askaway/android/
│   │       ├── MainActivity.kt        # 入口 Activity，管理多 Tab RN 容器
│   │       ├── MainApplication.kt     # Application，注册 RN Package
│   │       ├── MainViewModel.kt       # Tab 生命周期 ViewModel
│   │       └── core/
│   │           ├── ActrBridge.kt      # Actr SDK 桥接（WebRTC 信令/AI 通信）
│   │           ├── TabModule.kt       # RN NativeModule：Tab 操作
│   │           └── UserInfoModule.kt  # RN NativeModule：用户信息
│   └── settings.gradle       # Gradle 项目配置（含 autolink + 手动 include）
│
├── loom/                     # React Native 业务层（独立 package）
│   ├── src/
│   │   ├── TabRouter.tsx              # 顶级路由
│   │   ├── actr-host/                 # 核心宿主：多 Tab 管理、插件沙箱、UI 组件库、Zustand Store
│   │   ├── native-bridge/             # 原生通信层（Actr WebRTC）
│   │   ├── repository/               # 数据层（SQLite 持久化）
│   │   ├── services/                  # 编排服务（会话初始化、附件上传、资源下载）
│   │   ├── system-apps/              # 内置系统应用（设置页等）
│   │   ├── i18n/                     # 国际化
│   │   └── utils/                    # 工具函数（含 Pad/Phone 响应式布局 Token）
│   ├── AGENTS.md             # Loom 内部详细架构文档
│   └── package.json          # 独立依赖（react-native 0.79, zustand, reanimated 等）
│
├── index.js                  # RN 入口，注册 loom 为根组件
├── metro.config.js           # Metro 打包配置（双 node_modules 去重）
├── package.json              # 根依赖（RN 原生库在此声明以支持 autolink）
└── babel.config.js
```

## 外部依赖仓库（同级目录）

```
ipad/
├── Teal/           # 本仓库
├── actr-kotlin/    # Actr SDK（AI 通信引擎，Kotlin）
├── rill/           # Rill 插件沙箱引擎（JS 运行时隔离）
├── askit/          # 共享 TS 工具库
└── askaway-ipad/   # iPad 版本（独立项目）
```

## 关键架构决策

- **双 node_modules**：根目录 `node_modules` 供 Android autolink 使用，`loom/node_modules` 是 RN 业务层的实际依赖。Metro 通过 `extraNodeModules` + `blockList` 去重 react/react-native。
- **新架构 (New Architecture)**：已启用 (`newArchEnabled=true`)，使用 Hermes 引擎、TurboModules、Fabric。
- **RN 原生库链接**：大部分库通过根 `package.json` + autolink 自动链接；少数库（如 `react-native-safe-area-context`）在 `settings.gradle` 中手动 include。
- **Node 环境**：Gradle 构建时的 node 路径通过 `android/local.properties` 的 `reactNative.nodeExecutableAndArgs` 配置，解决 nvm 环境不可见问题。

## 构建注意事项

- 构建前确保 `android/local.properties` 中 `reactNative.nodeExecutableAndArgs` 指向正确的 node 绝对路径
- 新增 RN 原生库时，需同时添加到根 `package.json`（autolink）和 `loom/package.json`（JS 依赖）
- `android/build.gradle` 中 `REACT_NATIVE_NODE_MODULES_DIR` 为第三方库提供 react-native 路径，避免其内部调用 node 解析
