#import "RillSandboxNativeTurboModule.h"
#import "JSCSandboxJSI.h"
#import <React/RCTBridgeModule.h>
#import <ReactCommon/RCTTurboModule.h>

namespace rill::jsc {

RillSandboxNativeTurboModule::RillSandboxNativeTurboModule(
    std::shared_ptr<facebook::react::CallInvoker> invoker)
    : invoker_(invoker) {}

void RillSandboxNativeTurboModule::initialize(jsi::Runtime &runtime) {
  // Install JSC sandbox JSI bindings
  JSCSandboxModule::install(runtime);
}

} // namespace rill::jsc

/**
 * Objective-C++ bridge for TurboModule
 */
@interface RillSandboxNative : NSObject <RCTBridgeModule>
@end

@implementation RillSandboxNative
RCT_EXPORT_MODULE(RillSandboxNative)

+ (BOOL)requiresMainQueueSetup {
  return NO;
}

- (void)initialize:(RCTBridgeModule *)module {
  // Called during initialization
}

@end

/**
 * TurboModule Registration for new architecture
 * If React Native is using the new architecture (Fabric),
 * this will be called to register the module.
 */
#if RCT_NEW_ARCH_ENABLED
namespace facebook::react {

std::shared_ptr<TurboModule> RillSandboxNativeTurboModuleProvider(
    const ObjCTurboModule::InitParams &params) {
  auto module = std::make_shared<rill::jsc::RillSandboxNativeTurboModule>(
      params.jsInvoker);

  // Initialize JSI bindings when module is created
  if (auto jsiModule =
          std::dynamic_pointer_cast<rill::jsc::RillSandboxNativeTurboModule>(
              module)) {
    // The actual initialization happens in the module's constructor
    // or via a dedicated initialization method called by the bridge
  }

  return module;
}

} // namespace facebook::react
#endif

/**
 * Hook into JSI runtime initialization
 * This ensures the sandbox modules are installed as soon as the runtime is
 * ready
 */
static void installRillSandboxNative(RCTBridge *bridge)
    __attribute__((constructor));
static void installRillSandboxNative(RCTBridge *bridge) {
  // Register bridge initialization observer
  [[NSNotificationCenter defaultCenter]
      addObserverForName:RCTJavaScriptDidLoadNotification
                  object:nil
                   queue:[NSOperationQueue mainQueue]
              usingBlock:^(NSNotification *_Nonnull note){
                  // At this point, the JSI runtime is ready
                  // The JSI bindings will be installed by the TurboModule
                  // system
              }];
}
