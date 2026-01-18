#import "RillSandboxNativeTurboModule.h"

#import <objc/runtime.h>

#include <dispatch/dispatch.h>
#include <exception>
#include <mutex>

// Define fallbacks for macros that may not be defined
#ifndef RCT_NEW_ARCH_ENABLED
#define RCT_NEW_ARCH_ENABLED 0
#endif

// Forward declare sandbox install functions to avoid type conflicts
// QuickJS defines JSValue as a C struct, while React Native's RCTBridge.h
// forward declares it as an Objective-C class. Include sandbox headers
// in separate compilation units to avoid conflicts.
namespace quickjs_sandbox {
  void installQuickJSSandbox(facebook::jsi::Runtime &runtime);
}
namespace hermes_sandbox {
  void installHermesSandbox(facebook::jsi::Runtime &runtime);
}
namespace jsc_sandbox {
  void installJSCSandbox(facebook::jsi::Runtime &runtime);
}

#import <React/RCTBridgeModule.h>

#if !RCT_NEW_ARCH_ENABLED && __has_include(<React/RCTBridge+Private.h>)
#define RILL_HAS_RN_BRIDGE_HOOK 1
#import <React/RCTBridge+Private.h> // for runtime access
#import <objc/message.h>
#else
#define RILL_HAS_RN_BRIDGE_HOOK 0
#endif

#ifndef RILL_HAS_BRIDGING
#define RILL_HAS_BRIDGING 0
#endif

namespace {
std::mutex gInstallMutex;

#if RILL_SANDBOX_ENGINE == RILL_SANDBOX_ENGINE_QUICKJS
static constexpr const char *kSandboxGlobalName = "__QuickJSSandboxJSI";
static constexpr const char *kSandboxEngineName = "QuickJS";
#elif RILL_SANDBOX_ENGINE == RILL_SANDBOX_ENGINE_HERMES
static constexpr const char *kSandboxGlobalName = "__HermesSandboxJSI";
static constexpr const char *kSandboxEngineName = "Hermes";
#else
static constexpr const char *kSandboxGlobalName = "__JSCSandboxJSI";
static constexpr const char *kSandboxEngineName = "JSC";
#endif

static bool runtimeHasSandboxGlobal(facebook::jsi::Runtime &runtime) {
  try {
    return runtime.global().hasProperty(runtime, kSandboxGlobalName);
  } catch (...) {
    return false;
  }
}

static void installSandboxBindings(facebook::jsi::Runtime &runtime) {
#if RILL_SANDBOX_ENGINE == RILL_SANDBOX_ENGINE_QUICKJS
  quickjs_sandbox::installQuickJSSandbox(runtime);
#elif RILL_SANDBOX_ENGINE == RILL_SANDBOX_ENGINE_HERMES
  hermes_sandbox::installHermesSandbox(runtime);
#else
  jsc_sandbox::installJSCSandbox(runtime);
#endif
}

static void ensureSandboxInstalled(facebook::jsi::Runtime *runtime,
                                  const char *source) {
  if (runtime == nullptr) {
    NSLog(@"[RillSandboxNative] ensureSandboxInstalled called with null runtime (source=%s)",
          source ? source : "unknown");
    return;
  }

  std::lock_guard<std::mutex> lock(gInstallMutex);

  if (runtimeHasSandboxGlobal(*runtime)) {
    NSLog(@"[RillSandboxNative] %s sandbox JSI already installed (source=%s, runtime=%p)",
          kSandboxEngineName, source ? source : "unknown", runtime);
    return;
  }

  try {
    installSandboxBindings(*runtime);
    NSLog(@"[RillSandboxNative] Installed %s sandbox JSI (source=%s, runtime=%p)",
          kSandboxEngineName, source ? source : "unknown", runtime);
  } catch (const std::exception &e) {
    NSLog(@"[RillSandboxNative] Failed to install %s sandbox (source=%s): %s",
          kSandboxEngineName, source ? source : "unknown", e.what());
  } catch (...) {
    NSLog(@"[RillSandboxNative] Failed to install %s sandbox (source=%s): unknown error",
          kSandboxEngineName, source ? source : "unknown");
  }
}
} // namespace

#if RCT_NEW_ARCH_ENABLED
namespace {
using RCTHostDidInitializeRuntimeIMP =
    void (*)(id, SEL, id, facebook::jsi::Runtime &);
static RCTHostDidInitializeRuntimeIMP gOriginalRCTHostDidInitializeRuntime = nullptr;

static void rill_RCTHost_instance_didInitializeRuntime(id self,
                                                       SEL _cmd,
                                                       id instance,
                                                       facebook::jsi::Runtime &runtime) {
  ensureSandboxInstalled(&runtime, "RCTHost.instance:didInitializeRuntime:");
  if (gOriginalRCTHostDidInitializeRuntime != nullptr) {
    gOriginalRCTHostDidInitializeRuntime(self, _cmd, instance, runtime);
  }
}

static void installRCTHostRuntimeHookOnce() {
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    Class hostClass = NSClassFromString(@"RCTHost");
    if (hostClass == Nil) {
      NSLog(@"[RillSandboxNative] RCTHost not found; bridgeless auto-install disabled");
      return;
    }

    SEL selector = NSSelectorFromString(@"instance:didInitializeRuntime:");
    Method method = class_getInstanceMethod(hostClass, selector);
    if (method == nullptr) {
      NSLog(@"[RillSandboxNative] RCTHost missing selector instance:didInitializeRuntime:; bridgeless auto-install disabled");
      return;
    }

    IMP currentImp = method_getImplementation(method);
    if (currentImp == (IMP)rill_RCTHost_instance_didInitializeRuntime) {
      return;
    }

    IMP previousImp = method_setImplementation(
        method, (IMP)rill_RCTHost_instance_didInitializeRuntime);
    gOriginalRCTHostDidInitializeRuntime =
        (RCTHostDidInitializeRuntimeIMP)previousImp;

    NSLog(@"[RillSandboxNative] Installed RCTHost runtime hook (bridgeless auto-install)");
  });
}
} // namespace

static void installRillSandboxNativeRCTHostHook(void)
    __attribute__((constructor));
static void installRillSandboxNativeRCTHostHook(void) {
  installRCTHostRuntimeHookOnce();
}
#endif

#pragma mark - Public C API for bridgeless mode

extern "C" {

/**
 * Install sandbox JSI bindings into the given runtime.
 * Call this from RCTHostRuntimeDelegate::didInitializeRuntime in bridgeless mode.
 */
void RillSandboxNativeInstall(facebook::jsi::Runtime *runtime) {
  ensureSandboxInstalled(runtime, "RillSandboxNativeInstall");
}

} // extern "C"

namespace rill::sandbox_native {

RillSandboxNativeTurboModule::RillSandboxNativeTurboModule(
    std::shared_ptr<facebook::react::CallInvoker> invoker)
    : invoker_(invoker) {}

void RillSandboxNativeTurboModule::initialize(facebook::jsi::Runtime &runtime) {
  ensureSandboxInstalled(&runtime, "TurboModule.initialize");
}

} // namespace rill::sandbox_native

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
  auto module = std::make_shared<rill::sandbox_native::RillSandboxNativeTurboModule>(
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
 * Legacy (old RN architecture) auto-install hook:
 * Install sandbox JSI bindings right before JS starts executing, so JS can
 * detect globals immediately (DefaultProvider, etc.).
 */
#if RILL_HAS_RN_BRIDGE_HOOK && !RCT_NEW_ARCH_ENABLED
static void installRillSandboxNativeLegacyHook(void) __attribute__((constructor));
static void installRillSandboxNativeLegacyHook(void) {
  NSLog(@"[RillSandboxNative] legacy bridge hook enabled");
  // 安装时机调整到 JS 即将执行之前，确保 DefaultProvider 检测时 JSI 已写入 global
  [[NSNotificationCenter defaultCenter]
      addObserverForName:RCTJavaScriptWillStartExecutingNotification
                  object:nil
                   queue:[NSOperationQueue mainQueue]
              usingBlock:^(NSNotification *_Nonnull note) {
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
                ensureSandboxInstalled(rt, "RCTJavaScriptWillStartExecutingNotification");
              }];
}
#elif !RCT_NEW_ARCH_ENABLED
// Legacy (old RN architecture) but bridge runtime hook is unavailable (e.g.
// missing private headers). If you hit this, either enable the new
// architecture, or install the sandbox from your app with a custom runtime
// access path.
static void logLegacyHookUnavailable(void) __attribute__((constructor));
static void logLegacyHookUnavailable(void) {
  NSLog(@"[RillSandboxNative] legacy bridge hook unavailable: <React/RCTBridge+Private.h> not found");
}
#endif
