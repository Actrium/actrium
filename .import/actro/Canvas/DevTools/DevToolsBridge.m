#import <React/RCTBridgeModule.h>

@interface RCT_EXTERN_MODULE(DevToolsBridge, NSObject)

RCT_EXTERN_METHOD(logMessage:(NSString *)direction :(NSString *)event :(NSString *)payload :(NSString *)tabId)

@end
