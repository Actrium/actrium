# React Native 原生沙箱（JSI）集成

本页说明如何在 React Native 宿主运行时中安装 rill 的原生 JSI 沙箱（JSC / Hermes / QuickJS），以便 `DefaultProvider` 能自动检测并选择可用 Provider。

## 1. iOS/macOS：CocoaPods 集成

在 `Podfile` 中添加：

```rb
# 选择 Guest 引擎：'jsc'（默认）| 'hermes' | 'quickjs'
ENV['RILL_SANDBOX_ENGINE'] = 'quickjs'

pod 'RillSandboxNative', :path => '../node_modules/rill/native'
```

> 注意：`RILL_SANDBOX_ENGINE` 在编译期生效，需要重新 `pod install` / clean build。

## 2. Bridgeless（新架构 / New Architecture）

默认无需额外代码：`RillSandboxNative` 会在 Bridgeless runtime 初始化时自动安装 JSI 绑定。

如需显式控制/兜底（例如你希望明确安装点），可以在 `RCTHostRuntimeDelegate::didInitializeRuntime` 回调中调用 `RillSandboxNativeInstall(&runtime)`，并确保它发生在你创建 `Engine` 之前：

```objc
#import <RillSandboxNative/RillSandboxNativeTurboModule.h>

// 关键点：拿到 host runtime（facebook::jsi::Runtime&）并安装
- (void)host:(RCTHost *)host didInitializeRuntime:(facebook::jsi::Runtime &)runtime
{
  RillSandboxNativeInstall(&runtime);
}
```

不同 RN 版本的 Host/Delegate 组织方式可能不同，但“安装点”是同一个：**runtime 初始化完成后、JS 执行开始前**。

## 3. Bridge（旧架构）

默认启用 legacy auto-install hook：会在 `RCTJavaScriptWillStartExecutingNotification` 触发时自动安装 JSI 绑定，通常不需要 App 侧额外代码。

## 4. 排查清单

- JS 报错 `No native JSI sandbox module found` / `No suitable JS sandbox provider found`
  - Bridgeless：确认 Xcode/Console 里有 `[RillSandboxNative] Installed ... (source=RCTHost.instance:didInitializeRuntime:)`；必要时按上面方式手动调用一次 `RillSandboxNativeInstall(&runtime)`。
  - Bridge：确认 Xcode/Console 里看到 `[RillSandboxNative] Installed ...` 日志。
