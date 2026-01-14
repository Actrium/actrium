#import <React/RCTBridgeModule.h>

@interface RCT_EXTERN_MODULE(BookmarksModule, NSObject)

RCT_EXTERN_METHOD(openBookmark:(NSString *)bookmarkId :(NSString *)appURL)
RCT_EXTERN_METHOD(deleteBookmark:(NSString *)bookmarkId)

@end
