#import "RendererScrolling.h"

#import <React/RCTUIKit.h>
#import <React/RCTScrollViewComponentView.h>

static NSScrollView *scrollViewForHit(NSView *hitView)
{
    for (NSView *view = hitView; view != nil; view = view.superview) {
        NSScrollView *candidate = nil;
        if ([view isKindOfClass:NSScrollView.class]) {
            candidate = (NSScrollView *)view;
        } else if ([view isKindOfClass:RCTScrollViewComponentView.class]) {
            // Fabric can hit the component itself over padding. Its native
            // scroll view is a child, so the ancestor walk alone misses it.
            candidate = ((RCTScrollViewComponentView *)view).scrollView;
        }
        if (candidate != nil &&
            (![candidate isKindOfClass:RCTUIScrollView.class] || ((RCTUIScrollView *)candidate).isScrollEnabled)) {
            return candidate;
        }
    }
    return nil;
}

BOOL ActriumScrollRendererContent(NSView *hitView, NSPoint delta)
{
    NSScrollView *scrollView = scrollViewForHit(hitView);
    if (scrollView == nil) {
        return NO;
    }

    // The renderer window is offscreen. Moving the clip view directly also
    // posts the bounds notification that feeds React Native's scroll delegate.
    NSClipView *clip = scrollView.contentView;
    NSRect bounds = clip.bounds;
    bounds.origin.x -= delta.x;
    bounds.origin.y -= delta.y;
    [clip scrollToPoint:[clip constrainBoundsRect:bounds].origin];
    return YES;
}

BOOL ActriumSendEditingAction(SEL action, NSResponder *target)
{
    @try {
        return [NSApp sendAction:action to:target from:nil];
    } @catch (NSException *exception) {
        NSLog(@"Actrium: editing action %@ on %@ threw %@",
              NSStringFromSelector(action), target, exception);
        return NO;
    }
}
