import CoreFoundation
import CoreGraphics
import Foundation
import Testing
@testable import Actrium

struct CompactTabLayoutTests {
    @Test func tabStripCannotBecomeAWindowDragRegion() {
        let frames = compactToolbarInteractionFrames(
            in: CGRect(x: 0, y: 0, width: 1200, height: 45),
            isFullScreen: false,
            tabStripWidth: 640
        )
        #expect(frames.tabStrip.minX == 94)
        #expect(frames.containsInteractivePoint(CGPoint(x: 300, y: 2)))
        #expect(frames.containsInteractivePoint(CGPoint(x: 300, y: 43)))
        #expect(frames.containsInteractivePoint(CGPoint(x: 1140, y: 22)))
        #expect(frames.containsInteractivePoint(CGPoint(x: 1180, y: 22)))
        #expect(!frames.containsInteractivePoint(CGPoint(x: 900, y: 22)))
    }

    @Test func draggedTabUsesNearestLegalLandingSlot() {
        let candidates = [
            ToolbarTabChromeInsertionCandidate(preRemovalIndex: 0, idealOriginXs: [2]),
            ToolbarTabChromeInsertionCandidate(preRemovalIndex: 2, idealOriginXs: [424]),
        ]
        #expect(toolbarTabChromeInsertionIndex(
            draggedOriginX: 213,
            sourceIndex: 0,
            candidates: candidates
        ) == 0)
        #expect(toolbarTabChromeInsertionIndex(
            draggedOriginX: 214,
            sourceIndex: 0,
            candidates: candidates
        ) == 1)
    }

    @Test func draggedTabKeepsEarlierSlotOnExactTie() {
        let candidates = [
            ToolbarTabChromeInsertionCandidate(preRemovalIndex: 0, idealOriginXs: [48]),
            ToolbarTabChromeInsertionCandidate(preRemovalIndex: 2, idealOriginXs: [50]),
        ]
        #expect(toolbarTabChromeInsertionIndex(
            draggedOriginX: 49,
            sourceIndex: 1,
            candidates: candidates
        ) == 0)
    }

    @Test func addressEditingOwnsPointerUntilItEnds() {
        let rejected = toolbarTabInteractionTransition(
            phase: .addressEditing,
            input: .beginDrag
        )
        #expect(!rejected.accepted)
        #expect(rejected.phase == .addressEditing)
        #expect(!rejected.permitsTabDrag)

        let ended = toolbarTabInteractionTransition(
            phase: .addressEditing,
            input: .endAddressEditing
        )
        #expect(ended.accepted)
        #expect(ended.phase == .active)
    }

    @Test func contentFocusRevealsOnlyClippedActiveTabs() {
        #expect(toolbarTabRevealAlignment(
            tabFrame: CGRect(x: 24, y: 2, width: 180, height: 32),
            viewportWidth: 300
        ) == nil)
        #expect(toolbarTabRevealAlignment(
            tabFrame: CGRect(x: -24, y: 2, width: 180, height: 32),
            viewportWidth: 300
        ) == .leading)
        #expect(toolbarTabRevealAlignment(
            tabFrame: CGRect(x: 180, y: 2, width: 180, height: 32),
            viewportWidth: 300
        ) == .trailing)
    }

    @Test func widthsCompressBeforeTabsOverflow() {
        let active = UUID()
        let inactive = UUID()
        let inputs = [
            ToolbarAdaptiveTabWidthInput(id: active, preferredWidth: 200, minimumWidth: 100),
            ToolbarAdaptiveTabWidthInput(id: inactive, preferredWidth: 100, minimumWidth: 50),
        ]

        let compressed = toolbarAdaptiveTabWidths(inputs: inputs, availableWidth: 240)
        #expect(compressed[active] == 160)
        #expect(compressed[inactive] == 80)

        let overflowing = toolbarAdaptiveTabWidths(inputs: inputs, availableWidth: 100)
        #expect(overflowing[active] == 100)
        #expect(overflowing[inactive] == 50)
    }

    @Test func onlyNearestTabPinsAtEachEdge() {
        let hidden = UUID()
        let edge = UUID()
        let cover = UUID()
        let presentations = toolbarAccordionTabPresentations(
            inputs: [
                .init(id: hidden, frame: CGRect(x: -138, y: 0, width: 68, height: 32)),
                .init(id: edge, frame: CGRect(x: -20, y: 0, width: 68, height: 32)),
                .init(id: cover, frame: CGRect(x: 50, y: 0, width: 68, height: 32)),
            ],
            viewportWidth: 300
        )

        #expect(presentations[hidden] == .identity)
        #expect(presentations[edge]?.edge == .leading)
        #expect(presentations[edge]?.stackDepth == 2)
        #expect(presentations[edge]?.stackingZIndex == -10)
        #expect(presentations[cover]?.coverEdge == .leading)
        #expect(presentations[cover]?.stackingZIndex == 10)
    }

    @Test func foldedTabUnfoldsWithoutAJump() {
        let pinned = toolbarAccordionTabPresentation(
            tabFrame: CGRect(x: 1, y: 0, width: 68, height: 32),
            viewportWidth: 300
        )
        let almostOpen = toolbarAccordionTabPresentation(
            tabFrame: CGRect(x: 1.9, y: 0, width: 68, height: 32),
            viewportWidth: 300
        )
        let open = toolbarAccordionTabPresentation(
            tabFrame: CGRect(x: 2, y: 0, width: 68, height: 32),
            viewportWidth: 300
        )

        #expect(pinned.offsetX > almostOpen.offsetX)
        #expect(almostOpen.offsetX < 0.13)
        #expect(open.offsetX == 0)
    }

    @Test func stackContourDensityStaysBounded() {
        #expect(toolbarAccordionStackOffsets(stackDepth: 3) == [1, 2, 3])
        let deep = toolbarAccordionStackOffsets(stackDepth: 100)
        #expect(deep.count == 32)
        #expect(deep.last == 5)
    }
}
