#ifndef RILL_SANDBOX_NATIVE_TURBO_MODULE_H
#define RILL_SANDBOX_NATIVE_TURBO_MODULE_H

#include <jsi/jsi.h>
#include <ReactCommon/CallInvoker.h>

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

namespace rill::jsc {

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

} // namespace rill::jsc

#endif // RILL_SANDBOX_NATIVE_TURBO_MODULE_H
