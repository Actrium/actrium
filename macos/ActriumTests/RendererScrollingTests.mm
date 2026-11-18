#import <XCTest/XCTest.h>
#import <React/RCTScrollViewComponentView.h>
#import <react/renderer/components/scrollview/ScrollViewShadowNode.h>

#import "../ActriumViewService/RendererScrolling.h"

using namespace facebook::react;

@interface RendererScrollingTests : XCTestCase
@end

@implementation RendererScrollingTests

- (NSWindow *)mountList:(RCTScrollViewComponentView *)list
{
    auto data = std::make_shared<ScrollViewState>();
    data->contentBoundingRect = {{0, 0}, {300, 600}};
    auto state = std::make_shared<ScrollViewShadowNode::ConcreteState>(data, ShadowNodeFamily::Weak{});

    // Match Fabric's insert order. The content state arrives before the viewport layout.
    [list updateState:state oldState:nullptr];
    LayoutMetrics layout = {};
    layout.frame = {{0, 0}, {300, 300}};
    [list updateLayoutMetrics:layout oldLayoutMetrics:EmptyLayoutMetrics];
    RCTViewComponentView *content = [[RCTViewComponentView alloc] initWithFrame:NSMakeRect(16, 16, 268, 568)];
    [list mountChildComponentView:content index:0];

    NSWindow *window = [[NSWindow alloc] initWithContentRect:NSMakeRect(-100000, -100000, 300, 300)
        styleMask:NSWindowStyleMaskBorderless backing:NSBackingStoreBuffered defer:NO];
    window.releasedWhenClosed = NO;
    [window.contentView addSubview:list];
    [window orderOut:nil];
    [window.contentView layoutSubtreeIfNeeded];
    return window;
}

- (void)testFirstLayoutStopsAtTheContentBottomWithoutResizing
{
    RCTScrollViewComponentView *list = [[RCTScrollViewComponentView alloc] initWithFrame:NSZeroRect];
    NSWindow *window = [self mountList:list];
    NSClipView *clip = list.scrollView.contentView;
    NSView *hit = [list hitTest:NSMakePoint(40, 100)];
    XCTAssertNotNil(hit);
    XCTAssertTrue(ActriumScrollRendererContent(hit, NSMakePoint(0, -1000)));
    XCTAssertEqual(NSMinY(clip.bounds), 300);
    XCTAssertEqual(NSMaxY(clip.bounds), 600);
    [window close];
}

- (void)testPaddingAcceptsScrollInBothDirections
{
    RCTScrollViewComponentView *list = [[RCTScrollViewComponentView alloc] initWithFrame:NSZeroRect];
    NSWindow *window = [self mountList:list];
    NSPoint padding = NSMakePoint(8, 100);
    NSClipView *clip = list.scrollView.contentView;
    NSView *hit = [list hitTest:padding];
    XCTAssertEqualObjects(hit, list);
    XCTAssertTrue(ActriumScrollRendererContent(hit, NSMakePoint(0, -100)));
    XCTAssertEqual(NSMinY(clip.bounds), 100);
    XCTAssertTrue(ActriumScrollRendererContent([list hitTest:padding], NSMakePoint(0, 100)));
    XCTAssertEqual(NSMinY(clip.bounds), 0);
    [window close];
}

@end
