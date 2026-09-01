#import <React/RCTViewManager.h>
#import <React/RCTUIKit.h> // RCTUIView on macOS

@interface RNCSafeAreaProviderManager : RCTViewManager
@end

@implementation RNCSafeAreaProviderManager

RCT_EXPORT_MODULE(RNCSafeAreaProvider)

- (RCTUIView *)view
{
  // Simple placeholder realization: only as a normal container view
  RCTUIView *view = [RCTUIView new];
  view.wantsLayer = YES;
  view.layer.backgroundColor = [NSColor clearColor].CGColor;
  return view;
}

@end

