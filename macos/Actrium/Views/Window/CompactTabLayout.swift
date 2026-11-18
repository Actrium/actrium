import Foundation
import SwiftUI

enum CompactToolbarAction: CaseIterable, Hashable {
    case newTab
    case history
    case bookmarks
    case overview
}

struct CompactToolbarInteractionFrames {
    let tabStrip: CGRect
    let buttons: [CompactToolbarAction: CGRect]

    func containsInteractivePoint(_ point: CGPoint) -> Bool {
        tabStrip.contains(point)
            || buttons.values.contains(where: { $0.contains(point) })
    }
}

/// The SwiftUI toolbar and AppKit window-drag mask share this geometry.
func compactToolbarInteractionFrames(
    in bounds: CGRect,
    isFullScreen: Bool,
    tabStripWidth: CGFloat
) -> CompactToolbarInteractionFrames {
    CompactToolbarInteractionFrames(
        tabStrip: CompactToolbarLayout.tabStripFrame(
            in: bounds, isFullScreen: isFullScreen, contentWidth: tabStripWidth
        ),
        buttons: CompactToolbarLayout.actionButtonFrames(in: bounds)
    )
}

private enum ToolbarInPlaceDragSubject: Equatable {
    case tab(UUID)
    case group(UUID)
}

private struct ToolbarInPlaceDragState: Equatable {
    let subject: ToolbarInPlaceDragSubject
    let grabOffsetX: CGFloat
    var originX: CGFloat
}

enum ToolbarTabMotion {
    static let slideDuration: TimeInterval = 0.16
    static let addressContentsDelayNanoseconds: UInt64 = 175_000_000
    static let addressFocusDelayNanoseconds: UInt64 = 70_000_000
    static let slide = Animation.easeInOut(duration: slideDuration)
}

enum ToolbarTabRevealAlignment: Equatable {
    case leading
    case trailing
    case center
}

struct ToolbarAdaptiveTabWidthInput: Equatable {
    let id: UUID
    let preferredWidth: CGFloat
    let minimumWidth: CGFloat
}

/// Compress every visible Tab continuously before allowing the strip to
/// overflow. Each Tab gives up the same proportion of its own compressible
/// range, which keeps active/inactive Tabs visually balanced while respecting
/// their different usable minimum widths.
func toolbarAdaptiveTabWidths(
    inputs: [ToolbarAdaptiveTabWidthInput],
    availableWidth: CGFloat
) -> [UUID: CGFloat] {
    let normalized = inputs.map { input in
        let minimum = max(0, input.minimumWidth)
        return (
            id: input.id,
            preferred: max(minimum, input.preferredWidth),
            minimum: minimum
        )
    }
    let preferredTotal = normalized.reduce(CGFloat.zero) { $0 + $1.preferred }
    let minimumTotal = normalized.reduce(CGFloat.zero) { $0 + $1.minimum }
    let boundedAvailableWidth = max(0, availableWidth)

    if boundedAvailableWidth >= preferredTotal {
        return Dictionary(uniqueKeysWithValues: normalized.map { ($0.id, $0.preferred) })
    }
    if boundedAvailableWidth <= minimumTotal {
        return Dictionary(uniqueKeysWithValues: normalized.map { ($0.id, $0.minimum) })
    }

    let compressibleTotal = preferredTotal - minimumTotal
    guard compressibleTotal > 0 else {
        return Dictionary(uniqueKeysWithValues: normalized.map { ($0.id, $0.minimum) })
    }

    let compressionFraction = (preferredTotal - boundedAvailableWidth) / compressibleTotal
    return Dictionary(uniqueKeysWithValues: normalized.map { item in
        let compressibleWidth = item.preferred - item.minimum
        return (
            item.id,
            item.preferred - compressibleWidth * compressionFraction
        )
    })
}

enum ToolbarAccordionEdge: Equatable {
    case leading
    case trailing
}

enum ToolbarAccordionTabClickDisposition: Equatable {
    case activate
    case unfoldLeading
    case unfoldTrailing
}

struct ToolbarAccordionTabPresentation: Equatable {
    let offsetX: CGFloat
    let edge: ToolbarAccordionEdge?
    let collapseProgress: CGFloat
    let overlapProgress: CGFloat
    let coverEdge: ToolbarAccordionEdge?
    let stackDepth: Int

    init(
        offsetX: CGFloat,
        edge: ToolbarAccordionEdge?,
        collapseProgress: CGFloat,
        overlapProgress: CGFloat,
        coverEdge: ToolbarAccordionEdge? = nil,
        stackDepth: Int = 0
    ) {
        self.offsetX = offsetX
        self.edge = edge
        self.collapseProgress = collapseProgress
        self.overlapProgress = overlapProgress
        self.coverEdge = coverEdge
        self.stackDepth = max(0, stackDepth)
    }

    static let identity = ToolbarAccordionTabPresentation(
        offsetX: 0,
        edge: nil,
        collapseProgress: 0,
        overlapProgress: 0
    )

    func withStackDepth(_ stackDepth: Int) -> Self {
        Self(
            offsetX: offsetX,
            edge: edge,
            collapseProgress: collapseProgress,
            overlapProgress: overlapProgress,
            coverEdge: coverEdge,
            stackDepth: stackDepth
        )
    }

    var isFolded: Bool { edge != nil }
    var isLayered: Bool { isFolded || overlapProgress > 0 }
    var isObscured: Bool { isFolded }
    var isOverlapCover: Bool { coverEdge != nil && overlapProgress > 0 }

    /// Titles disappear before their Tab reaches a fully folded edge state.
    /// The edge Tab itself stays crisp underneath the incoming neighbour.
    var titleOpacity: Double {
        Double(max(0, 1 - collapseProgress * 2))
    }

    /// Every Tab remains opaque. Only the incoming cover's directional head
    /// gains a short transparent-to-opaque mask in the final 30% of overlap,
    /// when the two identity icons begin to converge.
    var coverHeadRevealProgress: CGFloat {
        guard isOverlapCover else { return 0 }
        return min(1, max(0, (overlapProgress - 0.7) / 0.3))
    }

    var coverHeadEdgeOpacity: Double {
        Double(1 - coverHeadRevealProgress)
    }

    var stackingZIndex: Double {
        if isFolded { return -10 }
        if overlapProgress > 0 { return 10 }
        return 0
    }

    var clickDisposition: ToolbarAccordionTabClickDisposition {
        switch edge {
        case .leading: .unfoldLeading
        case .trailing: .unfoldTrailing
        case nil: .activate
        }
    }
}

/// Preserves a one-point offset per accumulated Tab until the stack reaches
/// five points. Deeper stacks distribute their contour samples evenly inside
/// that fixed span, so subpixel overlap makes the edge progressively denser
/// without consuming more toolbar space or creating unbounded drawing work.
func toolbarAccordionStackOffsets(
    stackDepth: Int,
    maximumSpread: CGFloat = 5,
    maximumRenderedContours: Int = 32
) -> [CGFloat] {
    let depth = max(0, stackDepth)
    let spreadLimit = max(0, maximumSpread)
    let renderedDepth = min(depth, max(0, maximumRenderedContours))
    guard renderedDepth > 0, spreadLimit > 0 else { return [] }

    let spread = min(CGFloat(depth), spreadLimit)
    let step = spread / CGFloat(renderedDepth)
    return (1...renderedDepth).map { CGFloat($0) * step }
}

struct ToolbarAccordionTabLayoutInput: Equatable {
    let id: UUID
    let frame: CGRect
}

func toolbarTabRevealAlignment(
    tabFrame: CGRect,
    viewportWidth: CGFloat,
    edgeInset: CGFloat = 2
) -> ToolbarTabRevealAlignment? {
    guard viewportWidth > 0 else { return nil }
    let visibleMinX = max(0, edgeInset)
    let visibleMaxX = max(visibleMinX, viewportWidth - visibleMinX)
    let visibleWidth = visibleMaxX - visibleMinX
    let tolerance: CGFloat = 0.5
    if tabFrame.width > visibleWidth + tolerance { return .center }
    if tabFrame.minX < visibleMinX - tolerance { return .leading }
    if tabFrame.maxX > visibleMaxX + tolerance { return .trailing }
    return nil
}

func toolbarShouldDismissObscuredAddressFocus(
    presentation: ToolbarAccordionTabPresentation,
    isCurrentTab: Bool,
    isAddressEditing: Bool,
    showsFocusAffordances: Bool
) -> Bool {
    presentation.isObscured
        && isCurrentTab
        && isAddressEditing
        && showsFocusAffordances
}

/// Turns one overflowing Tab into a pinned, icon-bearing edge surface.
///
/// Folding begins with the first clipped pixel, rather than waiting until only
/// a narrow trailing slice remains. The real Tab then stays pinned while its
/// neighbour slides over it. Pinning is continuous, so reversing a
/// scroll peels the accordion open without a threshold jump.
func toolbarAccordionTabPresentation(
    tabFrame: CGRect,
    viewportWidth: CGFloat,
    edgeInset: CGFloat = 2
) -> ToolbarAccordionTabPresentation {
    guard viewportWidth > 0, tabFrame.width > 0 else {
        return .identity
    }

    let boundedInset = max(0, edgeInset)
    let leadingEdge = boundedInset
    let trailingEdge = max(leadingEdge, viewportWidth - boundedInset)
    let availableWidth = trailingEdge - leadingEdge
    guard tabFrame.width < availableWidth else { return .identity }

    // Width compression and ScrollGeometry can disagree by a few floating
    // point ulps when a Tab lands exactly on an edge. Ignore only that numeric
    // noise; a real tenth-point clip still starts folding continuously.
    let geometryEpsilon: CGFloat = 0.001
    let leadingOverflow = max(0, leadingEdge - tabFrame.minX - geometryEpsilon)
    let trailingOverflow = max(0, tabFrame.maxX - trailingEdge - geometryEpsilon)

    if leadingOverflow > 0, trailingOverflow > 0 {
        // Only malformed/intermediate resize geometry can cross both edges.
        // Keeping the logical slot avoids oscillating between accordions.
        return .identity
    }

    if leadingOverflow > 0 {
        return ToolbarAccordionTabPresentation(
            offsetX: leadingEdge - tabFrame.minX,
            edge: .leading,
            collapseProgress: min(1, leadingOverflow / tabFrame.width),
            overlapProgress: 0,
            stackDepth: 1
        )
    }

    if trailingOverflow > 0 {
        return ToolbarAccordionTabPresentation(
            offsetX: trailingEdge - tabFrame.maxX,
            edge: .trailing,
            collapseProgress: min(1, trailingOverflow / tabFrame.width),
            overlapProgress: 0,
            stackDepth: 1
        )
    }

    return .identity
}

/// Keeps at most one folded Tab visible at each viewport edge. When several
/// logical slots have already crossed an edge, the closest one owns the edge;
/// older slots remain normally clipped. This gives the strip one clean head
/// reveal instead of accumulating icon and surface layers.
func toolbarAccordionTabPresentations(
    inputs: [ToolbarAccordionTabLayoutInput],
    viewportWidth: CGFloat,
    edgeInset: CGFloat = 2
) -> [UUID: ToolbarAccordionTabPresentation] {
    let candidates = inputs.enumerated().map { index, input in
        (
            index: index,
            input: input,
            presentation: toolbarAccordionTabPresentation(
                tabFrame: input.frame,
                viewportWidth: viewportWidth,
                edgeInset: edgeInset
            )
        )
    }

    let leadingCandidates = candidates
        .filter { $0.presentation.edge == .leading }
    let leading = leadingCandidates
        .max { $0.input.frame.minX < $1.input.frame.minX }
    let trailingCandidates = candidates
        .filter { $0.presentation.edge == .trailing }
    let trailing = trailingCandidates
        .min { $0.input.frame.maxX < $1.input.frame.maxX }

    var result = Dictionary(
        uniqueKeysWithValues: inputs.map { ($0.id, ToolbarAccordionTabPresentation.identity) }
    )
    if let leading {
        result[leading.input.id] = leading.presentation.withStackDepth(
            leadingCandidates.count
        )
    }
    if let trailing {
        result[trailing.input.id] = trailing.presentation.withStackDepth(
            trailingCandidates.count
        )
    }

    let boundedInset = max(0, edgeInset)

    // At the leading edge, the Tab immediately to the right is the incoming
    // cover. It owns the sole directional head reveal and always draws above
    // the pinned edge Tab, independent of either Tab's active state.
    if let leading, leading.index + 1 < inputs.count {
        let cover = inputs[leading.index + 1]
        let pinnedMaxX = boundedInset + leading.input.frame.width
        let overlap = max(0, pinnedMaxX - cover.frame.minX)
        let overlapProgress = min(
            1,
            overlap / max(1, min(leading.input.frame.width, cover.frame.width))
        )
        if overlapProgress > 0, result[cover.id]?.isFolded != true {
            result[cover.id] = ToolbarAccordionTabPresentation(
                offsetX: 0,
                edge: nil,
                collapseProgress: 0,
                overlapProgress: overlapProgress,
                coverEdge: .leading
            )
        }
    }

    // The trailing edge is symmetric: the Tab immediately to the left is the
    // incoming cover, so logical order never changes the visual stacking or
    // directional head-mask rule.
    if let trailing, trailing.index > 0 {
        let cover = inputs[trailing.index - 1]
        let pinnedMinX = max(boundedInset, viewportWidth - boundedInset)
            - trailing.input.frame.width
        let overlap = max(0, cover.frame.maxX - pinnedMinX)
        let overlapProgress = min(
            1,
            overlap / max(1, min(trailing.input.frame.width, cover.frame.width))
        )
        if overlapProgress > 0, result[cover.id]?.isFolded != true {
            result[cover.id] = ToolbarAccordionTabPresentation(
                offsetX: 0,
                edge: nil,
                collapseProgress: 0,
                overlapProgress: overlapProgress,
                coverEdge: .trailing
            )
        }
    }
    return result
}
