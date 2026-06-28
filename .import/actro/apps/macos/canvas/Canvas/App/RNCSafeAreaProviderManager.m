#import <React/RCTViewManager.h>
#import <React/RCTUIKit.h> // RCTUIView on macOS

@interface RNCSafeAreaProviderManager : RCTViewManager
@end

@implementation RNCSafeAreaProviderManager

RCT_EXPORT_MODULE(RNCSafeAreaProvider)

- (RCTUIView *)view
{
  // 简单占位实现：仅作为普通容器视图使用
  RCTUIView *view = [RCTUIView new];
  view.wantsLayer = YES;
  view.layer.backgroundColor = [NSColor clearColor].CGColor;
  return view;
}

@end

