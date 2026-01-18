#ifndef RILL_SANDBOX_NATIVE_TURBO_MODULE_H
#define RILL_SANDBOX_NATIVE_TURBO_MODULE_H

#include <jsi/jsi.h>
#include <ReactCommon/CallInvoker.h>

// Engine constants (always defined)
#define RILL_SANDBOX_ENGINE_JSC 1
#define RILL_SANDBOX_ENGINE_HERMES 2
#define RILL_SANDBOX_ENGINE_QUICKJS 3

// Sandbox engine selection (default to JSC if not specified by build system)
#ifndef RILL_SANDBOX_ENGINE
#define RILL_SANDBOX_ENGINE RILL_SANDBOX_ENGINE_JSC
#endif

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Install sandbox JSI bindings into the given runtime.
 * Call this from RCTHostRuntimeDelegate::didInitializeRuntime in bridgeless mode.
 *
 * The sandbox engine (JSC or Hermes) is determined by RILL_SANDBOX_ENGINE at compile time.
 *
 * Usage in bridgeless app:
 *   - (void)host:(RCTHost *)host didInitializeRuntime:(facebook::jsi::Runtime &)runtime {
 *       RillSandboxNativeInstall(&runtime);
 *   }
 */
void RillSandboxNativeInstall(facebook::jsi::Runtime *runtime);

#ifdef __cplusplus
}
#endif

// React Native 0.79+ 在 macOS 上将 bridging 头文件放在 ReactCommon 路径下。
// 默认关闭 bridging 依赖，避免额外的 folly 生成文件；旧架构仅需要 CallInvoker。
#ifndef RILL_ENABLE_BRIDGING
#define RILL_ENABLE_BRIDGING 0
#endif

#if RILL_ENABLE_BRIDGING && __has_include(<ReactCommon/react/bridging/Bridging.h>)
#define RILL_HAS_BRIDGING 1
#include <ReactCommon/react/bridging/Bridging.h>
#elif RILL_ENABLE_BRIDGING && __has_include(<react/bridging/Bridging.h>)
#define RILL_HAS_BRIDGING 1
#include <react/bridging/Bridging.h>
#else
#define RILL_HAS_BRIDGING 0
#endif

namespace rill::sandbox_native {

/**
 * TurboModule for RillSandboxNative
 * Automatically initializes JSC and QuickJS sandboxes on app startup
 * Exposes global.__JSCSandboxJSI and global.__QuickJSSandboxJSI
 */
class RillSandboxNativeTurboModule
#if RILL_HAS_BRIDGING
    : public facebook::react::NativeModuleBase
#endif
{
public:
  RillSandboxNativeTurboModule(
      std::shared_ptr<facebook::react::CallInvoker> invoker);

  /**
   * Install JSI bindings into runtime
   * Called automatically by React Native
   */
  void initialize(facebook::jsi::Runtime &runtime);

private:
  std::shared_ptr<facebook::react::CallInvoker> invoker_;
};

} // namespace rill::sandbox_native

#endif // RILL_SANDBOX_NATIVE_TURBO_MODULE_H
