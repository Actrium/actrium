#import <React/RCTBridgeModule.h>
#import <React/RCTEventEmitter.h>

@interface RCT_EXTERN_MODULE(UserInfoModule, RCTEventEmitter)

// 暴露给 RN 的方法（可选）
RCT_EXTERN_METHOD(getCurrentUserInfo:(NSDictionary *)params
                  resolver:(RCTPromiseResolveBlock)resolve
                  rejecter:(RCTPromiseRejectBlock)reject)

RCT_EXTERN_METHOD(loginWithType:(NSString *)type)

@end
