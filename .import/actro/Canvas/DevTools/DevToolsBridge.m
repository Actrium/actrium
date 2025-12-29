#import <React/RCTBridgeModule.h>
#import <React/RCTEventEmitter.h>

@interface RCT_EXTERN_MODULE(DevToolsBridge, RCTEventEmitter)

RCT_EXTERN_METHOD(logMessage:(NSString *)direction :(NSString *)event :(NSString *)payload :(NSString *)tabId)
RCT_EXTERN_METHOD(logRenderOp:(NSString *)operationType :(NSString *)componentType :(NSString *)nodeId :(NSString *)payload :(NSString *)tabId)
RCT_EXTERN_METHOD(setEnginePaused:(BOOL)isPaused)
RCT_EXTERN_METHOD(registerPauseHandler)

@end
