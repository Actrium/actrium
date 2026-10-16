#import <React/RCTBridgeModule.h>
#import <React/RCTEventEmitter.h>

@interface RCT_EXTERN_MODULE(UserInfoModule, RCTEventEmitter)

// Exposure RN Method(s) (optional)
RCT_EXTERN_METHOD(getCurrentUserInfo:(NSDictionary *)params
                  resolver:(RCTPromiseResolveBlock)resolve
                  rejecter:(RCTPromiseRejectBlock)reject)

RCT_EXTERN_METHOD(loginWithType:(NSString *)type)

@end
