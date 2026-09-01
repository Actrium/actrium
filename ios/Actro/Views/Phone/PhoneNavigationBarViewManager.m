#import <React/RCTViewManager.h>

@interface RCT_EXTERN_MODULE(PhoneNavigationBarViewManager, RCTViewManager)

RCT_EXPORT_VIEW_PROPERTY(title, NSString)
RCT_EXPORT_VIEW_PROPERTY(subtitle, NSString)
RCT_EXPORT_VIEW_PROPERTY(showLargeTitle, BOOL)

@end
