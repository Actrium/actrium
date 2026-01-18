# React Native Native Sandbox (JSI) Integration

This page explains how to install Rill's native JSI sandboxes (JSC / Hermes / QuickJS) in a React Native host runtime, so that `DefaultProvider` can automatically detect and select the available Provider.

## 1. iOS/macOS: CocoaPods Integration

Add to your `Podfile`:

```rb
# Choose Guest engine: 'jsc' (default) | 'hermes' | 'quickjs'
ENV['RILL_SANDBOX_ENGINE'] = 'quickjs'

pod 'RillSandboxNative', :path => '../node_modules/rill/native'
```

> Note: `RILL_SANDBOX_ENGINE` takes effect at compile time, requiring `pod install` / clean build.

## 2. Bridgeless (New Architecture)

No additional code is needed by default: `RillSandboxNative` will automatically install JSI bindings during Bridgeless runtime initialization.

If you need explicit control/fallback (e.g., you want a specific installation point), you can call `RillSandboxNativeInstall(&runtime)` in the `RCTHostRuntimeDelegate::didInitializeRuntime` callback, ensuring it happens before you create your `Engine`:

```objc
#import <RillSandboxNative/RillSandboxNativeTurboModule.h>

// Key point: get the host runtime (facebook::jsi::Runtime&) and install
- (void)host:(RCTHost *)host didInitializeRuntime:(facebook::jsi::Runtime &)runtime
{
  RillSandboxNativeInstall(&runtime);
}
```

Different RN versions may organize Host/Delegate differently, but the "installation point" is the same: **after runtime initialization completes, before JS execution begins**.

## 3. Bridge (Legacy Architecture)

Legacy auto-install hook is enabled by default: it will automatically install JSI bindings when `RCTJavaScriptWillStartExecutingNotification` is triggered, usually requiring no additional app-side code.

## 4. Troubleshooting Checklist

- JS error `No native JSI sandbox module found` / `No suitable JS sandbox provider found`
  - Bridgeless: Confirm you see `[RillSandboxNative] Installed ... (source=RCTHost.instance:didInitializeRuntime:)` in Xcode/Console; if needed, manually call `RillSandboxNativeInstall(&runtime)` as shown above.
  - Bridge: Confirm you see `[RillSandboxNative] Installed ...` log in Xcode/Console.
