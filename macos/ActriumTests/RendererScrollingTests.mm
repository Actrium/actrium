#import <XCTest/XCTest.h>
#import <React/RCTScrollViewComponentView.h>
#import <react/renderer/components/scrollview/ScrollViewShadowNode.h>

#import "../ActriumViewService/RendererScrolling.h"

using namespace facebook::react;

// Observe the callback Fabric uses to publish contentOffset to its shadow tree.
@interface RCTScrollViewComponentView (ScrollDelegate) <RCTUIScrollViewDelegate>
@end

@interface OffsetRecordingScrollView : RCTScrollViewComponentView
@property (nonatomic) CGPoint reportedOffset;
@end

@implementation OffsetRecordingScrollView
- (void)scrollViewDidScroll:(RCTUIScrollView *)scrollView
{
    [super scrollViewDidScroll:scrollView];
    self.reportedOffset = scrollView.contentOffset;
}
@end

@interface RendererScrollingTests : XCTestCase
@end

@implementation RendererScrollingTests

- (void)testRemoteCursorFollowsTheNearestExplicitFabricStyle
{
    RCTViewComponentView *text = [[RCTViewComponentView alloc] initWithFrame:NSZeroRect];
    auto props = std::make_shared<ViewProps>();
    props->cursor = Cursor::Text;
    [text updateProps:props oldProps:[text props]];
    NSView *child = [NSView new];
    [text addSubview:child];
    XCTAssertEqualObjects(ActriumRendererCursorForHitView(child), @"text");
    XCTAssertEqualObjects(ActriumRendererCursorForHitView([NSView new]), @"default");

    RCTViewComponentView *button = [[RCTViewComponentView alloc] initWithFrame:NSZeroRect];
    auto buttonProps = std::make_shared<ViewProps>();
    buttonProps->cursor = Cursor::Pointer;
    [button updateProps:buttonProps oldProps:[button props]];
    [text addSubview:button];
    NSTextView *label = [NSTextView new];
    [button addSubview:label];
    XCTAssertEqualObjects(ActriumRendererCursorForHitView(label), @"pointer");
    XCTAssertEqualObjects(ActriumRendererCursorForHitView([NSTextView new]), @"text");
}

- (void)layOutList:(RCTScrollViewComponentView *)list
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
}

- (NSWindow *)mountList:(RCTScrollViewComponentView *)list
{
    [self layOutList:list];
    NSWindow *window = [[NSWindow alloc] initWithContentRect:NSMakeRect(-100000, -100000, 300, 300)
        styleMask:NSWindowStyleMaskBorderless backing:NSBackingStoreBuffered defer:NO];
    window.releasedWhenClosed = NO;
    [window.contentView addSubview:list];
    [window orderOut:nil];
    [window.contentView layoutSubtreeIfNeeded];
    return window;
}

- (void)testScrollPublishesTheOffsetUsedToLocateTextUnderTheMouse
{
    OffsetRecordingScrollView *list = [[OffsetRecordingScrollView alloc] initWithFrame:NSZeroRect];
    NSWindow *window = [self mountList:list];
    NSPoint pointer = NSMakePoint(40, 100);
    NSView *document = list.scrollView.documentView;
    for (NSNumber *delta in @[@0, @(-120), @45]) {
        XCTAssertTrue(ActriumScrollRendererContent(list, NSMakePoint(0, delta.doubleValue)));
        NSPoint nativePoint = [document convertPoint:pointer fromView:list];
        XCTAssertEqualWithAccuracy(pointer.y + list.reportedOffset.y, nativePoint.y, 0.01);
    }
    [window close];
}

- (void)testNestedScrollingPublishesBothOffsetsForTheSameMousePosition
{
    OffsetRecordingScrollView *outer = [[OffsetRecordingScrollView alloc] initWithFrame:NSZeroRect];
    NSWindow *window = [self mountList:outer];
    OffsetRecordingScrollView *inner = [[OffsetRecordingScrollView alloc] initWithFrame:NSZeroRect];
    [self layOutList:inner];
    [outer mountChildComponentView:inner index:1];
    [window.contentView layoutSubtreeIfNeeded];
    NSPoint pointer = NSMakePoint(40, 50);
    NSView *document = inner.scrollView.documentView;
    NSPoint before = [document convertPoint:pointer fromView:outer];
    XCTAssertTrue(ActriumScrollRendererContent(outer, NSMakePoint(0, -90)));
    XCTAssertTrue(ActriumScrollRendererContent(inner, NSMakePoint(0, -55)));
    NSPoint after = [document convertPoint:pointer fromView:outer];
    XCTAssertEqualWithAccuracy(after.y - before.y, 145, 0.01);
    XCTAssertEqualWithAccuracy(outer.reportedOffset.y, 90, 0.01);
    XCTAssertEqualWithAccuracy(inner.reportedOffset.y, 55, 0.01);
    [window close];
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
