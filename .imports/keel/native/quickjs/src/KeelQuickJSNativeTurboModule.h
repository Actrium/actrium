#ifndef KEEL_QUICKJS_NATIVE_TURBO_MODULE_H
#define KEEL_QUICKJS_NATIVE_TURBO_MODULE_H

#include <jsi/jsi.h>

namespace jsi = facebook::jsi;

namespace keel::quickjs {

/**
 * TurboModule for QuickJS sandbox
 * Initializes QuickJS sandbox on app startup
 * Exposes global.__QuickJSSandboxJSI
 */
class KeelQuickJSNativeTurboModule {
public:
  KeelQuickJSNativeTurboModule();
  ~KeelQuickJSNativeTurboModule();

  /**
   * Install JSI bindings into runtime
   */
  void initialize(jsi::Runtime &runtime);

private:
  bool initialized_;
};

} // namespace keel::quickjs

#endif // KEEL_QUICKJS_NATIVE_TURBO_MODULE_H
