# Keel C 代码集成与 TurboModuleRegistry 报错问题

---

## 1. 问题

本项目通过 ActroRuntime 的 React Native 模块以本地依赖的方式间接集成了 keel 项目。keel 项目包含 C 语言代码，但在当前项目编译过程中，keel 的 C 代码未被正确编译进最终产物。

为解决该集成问题，曾尝试在 app/gradle 配置中添加 keel 库 C 代码的路径，但运行时出现如下报错：

```
com.facebook.react.common.JavascriptException: TurboModuleRegistry.getEnforcing(...): 'PlatformConstants' could not be found. Verify that a module by this name is registered in the native binary., stack:
Invariant Violation: TurboModuleRegistry.getEnforcing(...): 'PlatformConstants' could not be found. Verify that a module by this name is registered in the native binary.
```

该问题的表现与 [React Native 官方 issue #47352](https://github.com/facebook/react-native/issues/47352) 完全一致。

---

## 2. 问题分析

该报错发生于 React Native 运行时通过 `TurboModuleRegistry.getEnforcing(...)` 获取名为 `PlatformConstants` 的 TurboModule 失败。

`PlatformConstants` 为 React Native 的核心模块。出现“核心模块找不到”并非因业务代码缺失自定义模块，而是由于当前应用实际加载的 native binary 及其初始化、注册流程与 React Native 预期不符，导致 TurboModule 未能正确注册或注册表不完整。

结合本项目的实际情况（为集成 keel 的 C 代码而调整 app/gradle 的 native 构建配置），可以明确认定，问题根源在于项目采用了自定义的 CMake 文件而未使用应用默认的 CMake 配置。这一变更导致 native binary 组合方式发生变化，进而影响 React Native 核心模块的注册与加载流程。

---

## 3. 解决方案

针对该问题，存在两种修复路径：

### 方案一：升级 React Native 版本

该问题属于 React Native 编译脚本的缺陷。官方已在较新版本中通过 [PR #47379](https://github.com/facebook/react-native/pull/47379/changes) 进行了修复。理论上，可通过升级 React Native 至包含该修复的版本以彻底解决此问题。

但受限于当前项目及相关依赖对 React Native 版本的兼容性要求，暂无法采用此升级方案。

### 方案二：预编译 keel C 代码为 .so 文件并放入 jniLibs（当前采用方案）

当前采用的方案为：先将 keel 项目的 C 代码编译为 .so 文件，再将其放置于 jniLibs 目录。

该方案无需修改 app 的 externalNativeBuild 或 CMake 配置，能够有效规避 React Native 核心模块丢失问题，确保 native binary 的组成与注册流程符合 React Native 预期。

---

## 4. 新问题

方案二虽可暂时解决集成问题，但每当 keel C 代码发生变更时，均需重新生成并替换对应的 .so 文件。该过程依赖人工操作，易导致构建产物与源码不一致（如遗漏更新 .so 或误用旧版本 .so），且无法实现源码级的即时迭代，影响开发效率与可维护性。
