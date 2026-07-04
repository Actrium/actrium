# Actro Android 客户端

## 项目概述

Actro Android 是 Actro 浏览器客户端的 Android 外壳，采用 **Kotlin 原生壳 + React Native 业务层** 的混合架构。原生壳负责多 Tab 窗口管理和系统级能力，RN 层（`packages/actro-runtime`）承载全部业务 UI 和逻辑。

## 仓库结构

```
apps/android/actro/
├── android/                  # Android 原生壳 (Kotlin)
│   ├── app/                  #   主应用模块
│   │   └── src/main/java/com/actro/android/
│   │       ├── MainActivity.kt        # 入口 Activity，管理多 Tab RN 容器
│   │       ├── MainApplication.kt     # Application，注册 RN Package
│   │       ├── MainViewModel.kt       # Tab 生命周期 ViewModel
│   │       └── core/
│   │           ├── ActrBridge.kt      # Actr SDK 桥接（WebRTC 信令/AI 通信）
│   │           ├── TabModule.kt       # RN NativeModule：Tab 操作
│   │           └── UserInfoModule.kt  # RN NativeModule：用户信息
│   └── settings.gradle       # Gradle 项目配置（含 autolink + 手动 include）
│
├── actro-runtime -> ../../../packages/actro-runtime
│                              # React Native 业务层软链，源码归位到 monorepo packages
│
├── index.js                  # RN 入口，注册 actro-runtime 为根组件
├── metro.config.js           # Metro 打包配置（双 node_modules 去重）
├── package.json              # 根依赖（RN 原生库在此声明以支持 autolink）
└── babel.config.js
```

## 共享依赖位置

```
packages/
├── actro-runtime/  # 共享 React Native 业务运行时
├── askit/          # 共享 TS 工具库
├── rill/           # Rill 插件沙箱引擎
└── supramark/      # Markdown / diagram 渲染能力
```

## 关键架构决策

- **双 node_modules**：Android 根目录 `node_modules` 供 Android autolink 使用，`packages/actro-runtime/node_modules` 是 RN 业务层的实际依赖。Metro 通过 `extraNodeModules` + `blockList` 去重 react/react-native。
- **新架构 (New Architecture)**：已启用 (`newArchEnabled=true`)，使用 Hermes 引擎、TurboModules、Fabric。
- **RN 原生库链接**：大部分库通过根 `package.json` + autolink 自动链接；少数库（如 `react-native-safe-area-context`）在 `settings.gradle` 中手动 include。
- **Node 环境**：Gradle 构建时的 node 路径通过 `android/local.properties` 的 `reactNative.nodeExecutableAndArgs` 配置，解决 nvm 环境不可见问题。

## 构建注意事项

- 构建前确保 `android/local.properties` 中 `reactNative.nodeExecutableAndArgs` 指向正确的 node 绝对路径
- 新增 RN 原生库时，需同时添加到 Android 根 `package.json`（autolink）和 `packages/actro-runtime/package.json`（JS 依赖）
- `android/build.gradle` 中 `REACT_NATIVE_NODE_MODULES_DIR` 为第三方库提供 react-native 路径，避免其内部调用 node 解析
