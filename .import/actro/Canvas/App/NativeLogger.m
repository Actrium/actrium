#import <React/RCTBridgeModule.h>

@interface RCT_EXTERN_MODULE(NativeLogger, NSObject)

RCT_EXTERN_METHOD(log:(NSString *)message)
RCT_EXTERN_METHOD(warn:(NSString *)message)
RCT_EXTERN_METHOD(error:(NSString *)message)

@end
