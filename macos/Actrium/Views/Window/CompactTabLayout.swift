import Foundation
import SwiftUI

enum CompactToolbarAction: CaseIterable, Hashable {
    case newTab
    case history
    case bookmarks
    case overview

    /// Actions the toolbar actually renders. History and Bookmarks are
    /// library pages; their header entry buttons stay hidden.
    static let visibleActions: [CompactToolbarAction] = [.newTab, .overview]
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
