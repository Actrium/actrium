#import <React/RCTBridgeModule.h>
#import <React/RCTEventEmitter.h>

@interface RCT_EXTERN_MODULE(DevToolsBridge, RCTEventEmitter)

RCT_EXTERN_METHOD(logMessage:(NSString *)direction :(NSString *)event :(NSString *)payload :(NSString *)tabId)
RCT_EXTERN_METHOD(logRenderOp:(NSString *)operationType :(NSString *)componentType :(NSString *)nodeId :(NSString *)payload :(NSString *)tabId)
RCT_EXTERN_METHOD(setEnginePaused:(BOOL)isPaused)
RCT_EXTERN_METHOD(registerPauseHandler)
RCT_EXTERN_METHOD(updateComponentTree:(NSString *)treeJson :(NSString *)tabId)
RCT_EXTERN_METHOD(updateEngineMonitor:(NSString *)snapshotJson)
RCT_EXTERN_METHOD(setInspectMode:(BOOL)enabled)
RCT_EXTERN_METHOD(highlightNode:(NSString *)nodeId :(NSDictionary *)frame)
RCT_EXTERN_METHOD(clearHighlight)
RCT_EXTERN_METHOD(selectNodeFromInspect:(NSString *)nodeId)
RCT_EXTERN_METHOD(setRNInspectorActive:(BOOL)active)
RCT_EXTERN_METHOD(updateBoxModel:(NSDictionary *)data)

@end
