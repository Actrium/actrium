//
//  KeelConfiguration.mm
//  ios-demo
//
//  Runtime configuration detection based on preprocessor macros set during pod install
//  Bridgeless-only: Bridge mode has been removed from this example.
//

#import "KeelConfiguration.h"

@implementation KeelConfiguration

+ (NSString *)modeName {
    return @"Bridgeless";
}

+ (NSString *)sandboxEngineName {
#if KEEL_SANDBOX_QUICKJS
    return @"QuickJS";
#elif KEEL_SANDBOX_HERMES
    return @"Hermes";
#elif KEEL_SANDBOX_JSC
    return @"JSC";
#else
    // Default to hermes if not specified
    return @"Hermes";
#endif
}

+ (BOOL)isBridgeless {
    return YES;
}

+ (BOOL)isJSCSandbox {
#if KEEL_SANDBOX_JSC
    return YES;
#else
    return NO;
#endif
}

+ (BOOL)isHermesSandbox {
#if KEEL_SANDBOX_HERMES
    return YES;
#else
    return NO;
#endif
}

+ (BOOL)isQuickJSSandbox {
#if KEEL_SANDBOX_QUICKJS
    return YES;
#else
    return NO;
#endif
}

@end
