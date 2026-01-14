#import <React/RCTViewManager.h>

@interface RNCSafeAreaProviderManager : RCTViewManager
@end

@implementation RNCSafeAreaProviderManager

RCT_EXPORT_MODULE(RNCSafeAreaProvider)

- (UIView *)view
{
  // 简单占位实现：仅作为普通容器视图使用
  UIView *view = [[UIView alloc] init];
  view.backgroundColor = [UIColor clearColor];
  return view;
}

@end

