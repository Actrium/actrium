#ifndef RILL_SANDBOX_NATIVE_TURBO_MODULE_H
#define RILL_SANDBOX_NATIVE_TURBO_MODULE_H

#include <react/bridging/Bridging.h>
#include <jsi/jsi.h>

namespace rill::jsc {

/**
 * TurboModule for RillSandboxNative
 * Automatically initializes JSC and QuickJS sandboxes on app startup
 * Exposes global.__JSCSandboxJSI and global.__QuickJSSandboxJSI
 */
class RillSandboxNativeTurboModule
    : public facebook::react::NativeModuleBase {
 public:
  RillSandboxNativeTurboModule(std::shared_ptr<facebook::react::CallInvoker> invoker);

  /**
   * Install JSI bindings into runtime
   * Called automatically by React Native
   */
  void initialize(jsi::Runtime& runtime);

 private:
  std::shared_ptr<facebook::react::CallInvoker> invoker_;
};

} // namespace rill::jsc

#endif // RILL_SANDBOX_NATIVE_TURBO_MODULE_H
