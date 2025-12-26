#import "RillSandboxNativeTurboModule.h"
#import "JSCSandboxJSI.h"
#import <React/RCTBridgeModule.h>
#import <React/RCTBridge+Private.h> // for runtime access
#import <objc/message.h>

// Define fallbacks for macros that may not be defined
#ifndef RCT_NEW_ARCH_ENABLED
#define RCT_NEW_ARCH_ENABLED 0
#endif

#ifndef RILL_HAS_BRIDGING
#define RILL_HAS_BRIDGING 0
#endif

namespace rill::jsc {

using namespace jsc_sandbox; // bring JSCSandboxModule into scope

RillSandboxNativeTurboModule::RillSandboxNativeTurboModule(
    std::shared_ptr<facebook::react::CallInvoker> invoker)
    : invoker_(invoker) {}

void RillSandboxNativeTurboModule::initialize(facebook::jsi::Runtime &runtime) {
  NSLog(@"[RillSandboxNative] initialize() called - installing JSI bindings");
  // Install JSC sandbox JSI bindings
  JSCSandboxModule::install(runtime);
  NSLog(@"[RillSandboxNative] JSI bindings installed successfully");
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

@end

/**
 * TurboModule Registration for new architecture
 * If React Native is using the new architecture (Fabric),
 * this will be called to register the module.
 */
#if RCT_NEW_ARCH_ENABLED && RILL_HAS_BRIDGING
namespace facebook::react {

std::shared_ptr<TurboModule> RillSandboxNativeTurboModuleProvider(
    const ObjCTurboModule::InitParams &params) {
  auto module = std::make_shared<rill::jsc::RillSandboxNativeTurboModule>(
      params.jsInvoker);

  NSLog(@"[RillSandboxNative] TurboModule provider called - new arch enabled");

  return module;
}

} // namespace facebook::react
#else
// Log if new architecture is not enabled
static void logArchStatus(void) __attribute__((constructor));
static void logArchStatus(void) {
  NSLog(@"[RillSandboxNative] TurboModule not enabled - RCT_NEW_ARCH_ENABLED=%d, RILL_HAS_BRIDGING=%d",
        RCT_NEW_ARCH_ENABLED, RILL_HAS_BRIDGING);
}
#endif

/**
 * Hook into JSI runtime initialization
 * This ensures the sandbox modules are installed as soon as the runtime is
 * ready
 */
static void installRillSandboxNative(void) __attribute__((constructor));
static void installRillSandboxNative(void) {
  NSLog(@"[RillSandboxNative] constructor invoked");
  // 安装时机调整到 JS 即将执行之前，确保 DefaultProvider 检测时 JSI 已写入 global
  [[NSNotificationCenter defaultCenter]
      addObserverForName:RCTJavaScriptWillStartExecutingNotification
                  object:nil
                   queue:[NSOperationQueue mainQueue]
              usingBlock:^(NSNotification *_Nonnull note) {
                static bool sInstalled = false;
                if (sInstalled) {
                  return;
                }

                RCTBridge *loadedBridge = (RCTBridge *)note.object;
                if (![loadedBridge isKindOfClass:[RCTBridge class]]) {
                  NSLog(@"[RillSandboxNative] Warning: WillStartExecuting object is not RCTBridge");
                  return;
                }

                NSLog(@"[RillSandboxNative] RCTBridge class: %@", NSStringFromClass([loadedBridge class]));
                NSLog(@"[RillSandboxNative] RCTBridge delegate: %@", loadedBridge.delegate);

                // 在 macOS 0.79 上 runtime 挂在 RCTCxxBridge 的 runtime 方法上
                RCTCxxBridge *cxxBridge = nil;
                if ([loadedBridge isKindOfClass:[RCTCxxBridge class]]) {
                  cxxBridge = (RCTCxxBridge *)loadedBridge;
                } else if ([loadedBridge respondsToSelector:@selector(batchedBridge)]) {
                  cxxBridge = (RCTCxxBridge *)[loadedBridge valueForKey:@"batchedBridge"];
                }

                if (cxxBridge == nil || ![cxxBridge respondsToSelector:@selector(runtime)]) {
                  NSLog(@"[RillSandboxNative] Error: Cannot access RCTCxxBridge runtime");
                  return;
                }

                void *runtimePtr = ((void *(*)(id, SEL))objc_msgSend)(cxxBridge, @selector(runtime));
                if (runtimePtr == nullptr) {
                  NSLog(@"[RillSandboxNative] Error: JS runtime is nil in WillStartExecuting, cannot install sandbox JSI");
                  return;
                }

                auto *rt =
                    reinterpret_cast<facebook::jsi::Runtime *>(runtimePtr);
                try {
                  jsc_sandbox::JSCSandboxModule::install(*rt);
                  NSLog(@"[RillSandboxNative] Installed JSC sandbox JSI (WillStartExecuting)");
                } catch (const std::exception &e) {
                  NSLog(@"[RillSandboxNative] Failed to install JSC sandbox: %s", e.what());
                }

                sInstalled = true;
              }];
}
