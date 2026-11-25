#if __has_include(<RNSVG/RNSVGSvgView.h>)
#import <RNSVG/RNSVGSvgView.h>

@implementation RNSVGSvgView (ActriumDrawing)

- (BOOL)wantsUpdateLayer
{
    // Fabric opts into updateLayer on macOS, but RNSVG paints in drawRect.
    // AppKit must draw the SVG into the layer exported by the Renderer.
    return NO;
}

@end
#endif
