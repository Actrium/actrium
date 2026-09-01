//
//  KeelSandboxBridge.h
//  ios-demo
//
//  Native module for sandbox performance measurement.
//  Registered as "KeelPerformanceBridge" to match the Android module name.
//  JS code uses NativeModules.KeelPerformanceBridge on both platforms.
//

#import <Foundation/Foundation.h>

NS_ASSUME_NONNULL_BEGIN

@interface KeelSandboxBridge : NSObject

+ (instancetype)sharedInstance;

@end

NS_ASSUME_NONNULL_END
