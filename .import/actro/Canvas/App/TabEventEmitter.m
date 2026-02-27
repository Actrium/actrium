#import <React/RCTBridgeModule.h>
#import <React/RCTEventEmitter.h>

@interface RCT_EXTERN_MODULE(TabEventEmitter, RCTEventEmitter)

RCT_EXTERN_METHOD(supportedEvents)

/// Exit fullscreen mode and return to tab list (iPhone only)
RCT_EXTERN_METHOD(exitFullscreen:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)

@end
