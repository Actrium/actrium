#import "AppDelegate.h"

#import <React/RCTBridgeModule.h>
#import <React/RCTBundleURLProvider.h>
#import <ReactAppDependencyProvider/RCTAppDependencyProvider.h>

// RillTestLogger - Native logging module for test output (stderr)
@interface RillTestLogger : NSObject <RCTBridgeModule>
@end

@implementation RillTestLogger

RCT_EXPORT_MODULE()

+ (BOOL)requiresMainQueueSetup {
  return NO;
}

// Use a blocking synchronous method so logs are flushed even if the app crashes.
RCT_EXPORT_BLOCKING_SYNCHRONOUS_METHOD(log:(NSString *)message)
{
  // Write directly to stderr (captured by terminal)
  fprintf(stderr, "%s\n", [message UTF8String]);
  fflush(stderr);
  return @YES;
}

@end

@implementation AppDelegate

- (void)applicationDidFinishLaunching:(NSNotification *)notification
{
  self.moduleName = @"RillMacOSTest";
  // You can add your custom initial props in the dictionary below.
  // They will be passed down to the ViewController used by React Native.
  NSString *sandboxTarget =
      [[NSProcessInfo processInfo] environment][@"RILL_SANDBOX_TARGET"];
  if (sandboxTarget != nil && [sandboxTarget length] > 0) {
    self.initialProps = @{@"rillSandbox" : sandboxTarget};
  } else {
    self.initialProps = @{};
  }
  self.dependencyProvider = [RCTAppDependencyProvider new];
  
  return [super applicationDidFinishLaunching:notification];
}

- (NSURL *)sourceURLForBridge:(RCTBridge *)bridge
{
  return [self bundleURL];
}

- (NSURL *)bundleURL
{
  NSString *useBundled =
      [[NSProcessInfo processInfo] environment][@"RILL_E2E_USE_BUNDLED_JS"];
  if (useBundled != nil && [useBundled length] > 0 &&
      ![useBundled isEqualToString:@"0"]) {
    NSURL *url = [[NSBundle mainBundle] URLForResource:@"main"
                                        withExtension:@"jsbundle"];
    if (url != nil) {
      return url;
    }
  }

#if DEBUG
  return [[RCTBundleURLProvider sharedSettings] jsBundleURLForBundleRoot:@"index"];
#else
  return [[NSBundle mainBundle] URLForResource:@"main" withExtension:@"jsbundle"];
#endif
}

/// This method controls whether the `concurrentRoot`feature of React18 is turned on or off.
///
/// @see: https://reactjs.org/blog/2022/03/29/react-v18.html
/// @note: This requires to be rendering on Fabric (i.e. on the New Architecture).
/// @return: `true` if the `concurrentRoot` feature is enabled. Otherwise, it returns `false`.
- (BOOL)concurrentRootEnabled
{
#ifdef RN_FABRIC_ENABLED
  return true;
#else
  return false;
#endif
}

@end
