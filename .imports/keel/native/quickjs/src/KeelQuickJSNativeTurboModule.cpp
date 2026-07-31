#include "KeelQuickJSNativeTurboModule.h"
#include "QuickJSSandboxJSI.h"

namespace keel::quickjs {

KeelQuickJSNativeTurboModule::KeelQuickJSNativeTurboModule()
    : initialized_(false) {}

KeelQuickJSNativeTurboModule::~KeelQuickJSNativeTurboModule() {}

void KeelQuickJSNativeTurboModule::initialize(jsi::Runtime &runtime) {
  if (initialized_) {
    return;
  }

  // Install QuickJS sandbox JSI bindings
  quickjs_sandbox::QuickJSSandboxModule::install(runtime);

  initialized_ = true;
}

} // namespace keel::quickjs
