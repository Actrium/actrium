#import <React/RCTBridgeModule.h>

@interface RCT_EXTERN_MODULE(HistoryModule, NSObject)

RCT_EXTERN_METHOD(openHistoryItem:(NSString *)itemId :(NSString *)appURL)
RCT_EXTERN_METHOD(deleteHistoryItem:(NSString *)itemId)
RCT_EXTERN_METHOD(restoreGroup:(NSString *)groupId)
RCT_EXTERN_METHOD(clearHistory)

@end
