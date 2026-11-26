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
        #expect(frames.containsInteractivePoint(CGPoint(x: 300, y: 6)))
        #expect(frames.containsInteractivePoint(CGPoint(x: 300, y: 40)))
        #expect(!frames.containsInteractivePoint(CGPoint(x: 300, y: 2)))
        #expect(!frames.containsInteractivePoint(CGPoint(x: 900, y: 22)))
    }

    @Test func headerButtonsKeepTheirOrderAndReserveSpaceInANarrowWindow() throws {
        let frames = compactToolbarInteractionFrames(
            in: CGRect(x: 0, y: 0, width: 640, height: 45),
            isFullScreen: false,
            tabStripWidth: 640
        )
        let newTab = try #require(frames.buttons[.newTab])
        let overview = try #require(frames.buttons[.overview])

        // History and Bookmarks are library pages now; their header entries
        // must not own a hit area.
        #expect(frames.buttons[.history] == nil)
        #expect(frames.buttons[.bookmarks] == nil)

        #expect(frames.tabStrip.maxX < newTab.minX)
        #expect(newTab.maxX < overview.minX)
        #expect(overview.minX - newTab.maxX == CompactToolbarLayout.itemSpacing)
        for frame in [newTab, overview] {
            #expect(frames.containsInteractivePoint(CGPoint(x: frame.midX, y: frame.midY)))
            #expect(!frames.containsInteractivePoint(CGPoint(x: frame.midX, y: 2)))
        }
        #expect(!frames.containsInteractivePoint(
            CGPoint(x: (newTab.maxX + overview.minX) / 2, y: newTab.midY)
        ))
    }

    @Test @MainActor func headerActionsCreateOrRefocusTheMatchingSystemTab() {
        let viewModel = BrowserViewModel(saveBrowserSession: { _, _ in })
        for type: SystemTabType in [.history, .bookmarks] {
            let first = viewModel.openSystemTab(type)
            _ = viewModel.addNewTab(focusAddressBar: false)
            viewModel.requestAddressBarFocus()
            let tabIDs = viewModel.tabs.map(\.id)
            let selected = viewModel.openSystemTab(type)

            #expect(selected.id == first.id)
            #expect(viewModel.tabs.map(\.id) == tabIDs)
            #expect(viewModel.currentTab?.url == type.aboutURL)
            #expect(viewModel.currentTabId == first.id)
            #expect(viewModel.actriumMode == .focus(tabId: first.id))
            #expect(!viewModel.shouldAutoFocusAddressBar)
        }
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

    @Test func edgeGroupsExposeOutsideUngroupSlots() {
        let group = UUID()
        let leading = [group, group, nil] as [UUID?]
        let trailing = [nil, group, group] as [UUID?]
        let middle = [nil, group, nil] as [UUID?]

        #expect(toolbarEdgeUngroupInsertionIndex(
            pointerX: 6,
            viewportWidth: 300,
            sourceGroupID: group,
            tabGroupIDs: leading
        ) == 0)
        #expect(toolbarEdgeUngroupInsertionIndex(
            pointerX: 294,
            viewportWidth: 300,
            sourceGroupID: group,
            tabGroupIDs: trailing
        ) == 2)
        #expect(toolbarEdgeUngroupInsertionIndex(
            pointerX: 24,
            viewportWidth: 300,
            sourceGroupID: group,
            tabGroupIDs: leading
        ) == nil)
        #expect(toolbarEdgeUngroupInsertionIndex(
            pointerX: 300,
            viewportWidth: 300,
            sourceGroupID: group,
            tabGroupIDs: middle
        ) == nil)
    }

    @Test func edgeUngroupZoneOutranksTheGroupUnderThePointer() {
        let group = UUID()
        // An expanded leading group painted from the strip's content inset
        // (3 pt) reaches into the 8 pt ungroup zone.
        let frames = [group: CGRect(x: 3, y: 0, width: 200, height: 32)]
        let tabGroupIDs = [group, group, nil] as [UUID?]

        // Inside the overlap a release ungroups, so the group must not be
        // highlighted even though its frame covers the pointer.
        #expect(toolbarDropTargetGroupID(
            pointerX: 5,
            viewportWidth: 400,
            sourceGroupID: group,
            tabGroupIDs: tabGroupIDs,
            groupFrames: frames,
            expandedGroupIDs: [group]
        ) == nil)

        // Past the zone the same group is the drop target.
        #expect(toolbarDropTargetGroupID(
            pointerX: 40,
            viewportWidth: 400,
            sourceGroupID: group,
            tabGroupIDs: tabGroupIDs,
            groupFrames: frames,
            expandedGroupIDs: [group]
        ) == group)
    }

    @Test func trailingEdgeUngroupsBeforeHighlightingTheGroup() {
        let group = UUID()
        let frames = [group: CGRect(x: 200, y: 0, width: 197, height: 32)]
        let tabGroupIDs = [nil, group, group] as [UUID?]

        #expect(toolbarDropTargetGroupID(
            pointerX: 395,
            viewportWidth: 400,
            sourceGroupID: group,
            tabGroupIDs: tabGroupIDs,
            groupFrames: frames,
            expandedGroupIDs: [group]
        ) == nil)
        #expect(toolbarDropTargetGroupID(
            pointerX: 300,
            viewportWidth: 400,
            sourceGroupID: group,
            tabGroupIDs: tabGroupIDs,
            groupFrames: frames,
            expandedGroupIDs: [group]
        ) == group)
    }

    @Test func onlyExpandedGroupsOwnThePointer() {
        let group = UUID()
        let other = UUID()
        let frames = [group: CGRect(x: 3, y: 0, width: 60, height: 32)]

        // A collapsed group owns a header frame but never accepts a Tab.
        #expect(toolbarDropTargetGroupID(
            pointerX: 40,
            viewportWidth: 400,
            sourceGroupID: nil,
            tabGroupIDs: [nil, nil],
            groupFrames: frames,
            expandedGroupIDs: []
        ) == nil)

        // The ungroup slot only releases a Tab that belongs to the group at
        // that end, so another Tab dragged into the same zone still joins the
        // group painted under the pointer.
        #expect(toolbarDropTargetGroupID(
            pointerX: 5,
            viewportWidth: 400,
            sourceGroupID: other,
            tabGroupIDs: [group, group, other],
            groupFrames: frames,
            expandedGroupIDs: [group, other]
        ) == group)
    }

    @Test func groupBoundaryReorderWaitsForPointerTravel() {
        #expect(toolbarDragCanReorder(pointerX: 100, lastReorderPointerX: nil))
        #expect(!toolbarDragCanReorder(pointerX: 100, lastReorderPointerX: 100))
        #expect(!toolbarDragCanReorder(pointerX: 105, lastReorderPointerX: 100))
        #expect(toolbarDragCanReorder(pointerX: 106, lastReorderPointerX: 100))
        #expect(toolbarDragCanReorder(pointerX: 94, lastReorderPointerX: 100))
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

    @Test @MainActor func dragPresentationHasOnlyOneActiveTab() {
        let draggedTab = UUID()
        let currentTab = UUID()
        let interaction = ToolbarTabInteractionCoordinator()

        #expect(interaction.beginDrag(tabID: draggedTab, activeTabID: draggedTab))
        #expect(interaction.phase(for: draggedTab, activeTabID: currentTab) == .draggingInactive)
        #expect(interaction.phase(for: currentTab, activeTabID: currentTab) == .active)

        interaction.pointerSequenceEnded(activeTabID: currentTab)
        #expect(!interaction.isDragging)
        #expect(interaction.phase(for: draggedTab, activeTabID: currentTab) == .inactive)
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
}
