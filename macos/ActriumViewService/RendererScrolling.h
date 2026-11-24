#import <AppKit/AppKit.h>

NS_ASSUME_NONNULL_BEGIN

FOUNDATION_EXPORT BOOL ActriumScrollRendererContent(NSView *hitView, NSPoint delta);
FOUNDATION_EXPORT NSString *ActriumRendererCursorForHitView(NSView * _Nullable hitView);
FOUNDATION_EXPORT BOOL ActriumRendererTracksHover(NSView *view);

/// Runs an AppKit editing action with an ObjC exception barrier. Undo
/// operations registered for IME-mirrored text can go stale once RN
/// rewrites the text storage; their exceptions must not crash the process.
FOUNDATION_EXPORT BOOL ActriumSendEditingAction(SEL action, NSResponder *target);

NS_ASSUME_NONNULL_END
