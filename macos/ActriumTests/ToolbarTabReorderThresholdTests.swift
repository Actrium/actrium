import CoreGraphics
import Foundation
import Testing
@testable import Actro

@MainActor
struct ToolbarTabReorderThresholdTests {
    @Test func chromeRule_selectsTheNearestIdealSlot() {
        let candidates = twoTabChromeCandidates()

        #expect(toolbarTabChromeInsertionIndex(
            draggedOriginX: 50,
            sourceIndex: 1,
            candidates: candidates
        ) == 1)
        #expect(toolbarTabChromeInsertionIndex(
            draggedOriginX: 48,
            sourceIndex: 1,
            candidates: candidates
        ) == 0)
    }

    @Test func chromeRule_keepsTheEarlierCandidateOnAnExactTie() {
        let candidates = twoTabChromeCandidates()

        #expect(toolbarTabChromeInsertionIndex(
            draggedOriginX: 49,
            sourceIndex: 1,
            candidates: candidates
        ) == 0)
    }

    @Test func chromeRule_usesLandingPositionsRatherThanSymmetricOverlap() {
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

    @Test func bodyFocus_keepsAFullyVisibleActiveTabStationary() {
        #expect(toolbarTabRevealAlignment(
            tabFrame: CGRect(x: 24, y: 2, width: 180, height: 32),
            viewportWidth: 300
        ) == nil)
    }

    @Test func bodyFocus_revealsTheClippedEdgeOfTheActiveTab() {
        #expect(toolbarTabRevealAlignment(
            tabFrame: CGRect(x: -24, y: 2, width: 180, height: 32),
            viewportWidth: 300
        ) == .leading)
        #expect(toolbarTabRevealAlignment(
            tabFrame: CGRect(x: 180, y: 2, width: 180, height: 32),
            viewportWidth: 300
        ) == .trailing)
    }

    @Test func bodyFocus_centersAnActiveTabWiderThanTheViewport() {
        #expect(toolbarTabRevealAlignment(
            tabFrame: CGRect(x: -80, y: 2, width: 420, height: 32),
            viewportWidth: 300
        ) == .center)
    }

    @Test func adaptiveLayout_keepsPreferredWidthsWhileEverythingFits() {
        let firstID = UUID()
        let secondID = UUID()
        let widths = toolbarAdaptiveTabWidths(
            inputs: [
                ToolbarAdaptiveTabWidthInput(
                    id: firstID,
                    preferredWidth: 200,
                    minimumWidth: 100
                ),
                ToolbarAdaptiveTabWidthInput(
                    id: secondID,
                    preferredWidth: 100,
                    minimumWidth: 50
                ),
            ],
            availableWidth: 320
        )

        #expect(widths[firstID] == 200)
        #expect(widths[secondID] == 100)
    }

    @Test func adaptiveLayout_compressesProportionallyBeforeOverflowing() {
        let firstID = UUID()
        let secondID = UUID()
        let widths = toolbarAdaptiveTabWidths(
            inputs: [
                ToolbarAdaptiveTabWidthInput(
                    id: firstID,
                    preferredWidth: 200,
                    minimumWidth: 100
                ),
                ToolbarAdaptiveTabWidthInput(
                    id: secondID,
                    preferredWidth: 100,
                    minimumWidth: 50
                ),
            ],
            availableWidth: 240
        )

        #expect(widths[firstID] == 160)
        #expect(widths[secondID] == 80)
    }

    @Test func adaptiveLayout_reachesEveryMinimumBeforeScrolling() {
        let activeID = UUID()
        let inactiveID = UUID()
        let widths = toolbarAdaptiveTabWidths(
            inputs: [
                ToolbarAdaptiveTabWidthInput(
                    id: activeID,
                    preferredWidth: 420,
                    minimumWidth: 220
                ),
                ToolbarAdaptiveTabWidthInput(
                    id: inactiveID,
                    preferredWidth: 120,
                    minimumWidth: 68
                ),
            ],
            availableWidth: 200
        )

        #expect(widths[activeID] == 220)
        #expect(widths[inactiveID] == 68)
    }

    @Test func accordionLayout_keepsFullyVisibleTabsAtTheirLogicalPosition() {
        #expect(toolbarAccordionTabOffset(
            tabFrame: CGRect(x: 80, y: 2, width: 68, height: 32),
            viewportWidth: 300
        ) == 0)

        #expect(toolbarAccordionTabOffset(
            tabFrame: CGRect(x: 230.000_000_1, y: 2, width: 68, height: 32),
            viewportWidth: 300
        ) == 0)
    }

    @Test func accordionLayout_keepsOnlyTheNearestLeadingTabFolded() {
        let deepestID = UUID()
        let nearestID = UUID()
        let visibleID = UUID()
        let presentations = toolbarAccordionTabPresentations(
            inputs: [
                .init(id: deepestID, frame: CGRect(x: -138, y: 2, width: 68, height: 32)),
                .init(id: nearestID, frame: CGRect(x: -20, y: 2, width: 68, height: 32)),
                .init(id: visibleID, frame: CGRect(x: 50, y: 2, width: 68, height: 32)),
            ],
            viewportWidth: 300
        )

        #expect(presentations[deepestID] == .identity)
        #expect(presentations[deepestID]?.isLayered == false)
        #expect(presentations[nearestID]?.edge == .leading)
        #expect(presentations[nearestID]?.isLayered == true)
        #expect(presentations[nearestID]?.isObscured == true)
        #expect(presentations[nearestID]?.surfaceOpacity == 1)
        #expect(presentations[nearestID]?.stackingZIndex == -10)
        #expect((presentations[visibleID]?.overlapProgress ?? 0) > 0)
        #expect(presentations[visibleID]?.isLayered == true)
        #expect(presentations[visibleID]?.isObscured == false)
        #expect((presentations[visibleID]?.surfaceOpacity ?? 1) < 1)
        #expect(presentations[visibleID]?.stackingZIndex == 10)
        #expect(-20 + (presentations[nearestID]?.offsetX ?? 0) == 2)
    }

    @Test func accordionLayout_keepsOnlyTheNearestTrailingTabFolded() {
        let visibleID = UUID()
        let nearestID = UUID()
        let deepestID = UUID()
        let presentations = toolbarAccordionTabPresentations(
            inputs: [
                .init(id: visibleID, frame: CGRect(x: 180, y: 2, width: 68, height: 32)),
                .init(id: nearestID, frame: CGRect(x: 260, y: 2, width: 68, height: 32)),
                .init(id: deepestID, frame: CGRect(x: 330, y: 2, width: 68, height: 32)),
            ],
            viewportWidth: 300
        )

        #expect(presentations[nearestID]?.edge == .trailing)
        #expect(presentations[nearestID]?.isLayered == true)
        #expect(presentations[nearestID]?.isObscured == true)
        #expect(presentations[nearestID]?.surfaceOpacity == 1)
        #expect(presentations[nearestID]?.stackingZIndex == -10)
        #expect((presentations[visibleID]?.overlapProgress ?? 0) > 0)
        #expect(presentations[visibleID]?.isLayered == true)
        #expect(presentations[visibleID]?.isObscured == false)
        #expect((presentations[visibleID]?.surfaceOpacity ?? 1) < 1)
        #expect(presentations[visibleID]?.stackingZIndex == 10)
        #expect(presentations[deepestID] == .identity)
        #expect(260 + 68 + (presentations[nearestID]?.offsetX ?? 0) == 298)
    }

    @Test func accordionLayout_unfoldsContinuouslyAsATabReenters() {
        let pinnedOffset = toolbarAccordionTabOffset(
            tabFrame: CGRect(x: 1, y: 2, width: 68, height: 32),
            viewportWidth: 300
        )
        let nearlyOpenOffset = toolbarAccordionTabOffset(
            tabFrame: CGRect(x: 1.9, y: 2, width: 68, height: 32),
            viewportWidth: 300
        )
        let openOffset = toolbarAccordionTabOffset(
            tabFrame: CGRect(x: 2, y: 2, width: 68, height: 32),
            viewportWidth: 300
        )

        #expect(pinnedOffset > nearlyOpenOffset)
        #expect(nearlyOpenOffset < 0.13)
        #expect(openOffset == 0)
    }

    @Test func accordionLayout_startsWithTheFirstClippedPixelAndSoftensOneSurface() {
        let barelyFolded = toolbarAccordionTabPresentation(
            tabFrame: CGRect(x: 1.9, y: 2, width: 68, height: 32),
            viewportWidth: 300
        )
        let foldedID = UUID()
        let coverID = UUID()
        let presentations = toolbarAccordionTabPresentations(
            inputs: [
                .init(id: foldedID, frame: CGRect(x: -40, y: 2, width: 68, height: 32)),
                .init(id: coverID, frame: CGRect(x: 30, y: 2, width: 68, height: 32)),
            ],
            viewportWidth: 300
        )

        #expect(barelyFolded.isFolded)
        #expect(barelyFolded.collapseProgress > 0)
        #expect(presentations[foldedID]?.titleOpacity == 0)
        #expect(presentations[foldedID]?.surfaceOpacity == 1)
        #expect((presentations[coverID]?.surfaceOpacity ?? 1) > 0.7)
        #expect((presentations[coverID]?.surfaceOpacity ?? 1) < 1)
    }

    @Test func accordionLayout_foldedClicksOnlyUnfoldTheirEdge() {
        let leading = toolbarAccordionTabPresentation(
            tabFrame: CGRect(x: -10, y: 2, width: 68, height: 32),
            viewportWidth: 300
        )
        let trailing = toolbarAccordionTabPresentation(
            tabFrame: CGRect(x: 250, y: 2, width: 68, height: 32),
            viewportWidth: 300
        )
        let complete = toolbarAccordionTabPresentation(
            tabFrame: CGRect(x: 80, y: 2, width: 68, height: 32),
            viewportWidth: 300
        )

        #expect(leading.clickDisposition == .unfoldLeading)
        #expect(trailing.clickDisposition == .unfoldTrailing)
        #expect(complete.clickDisposition == .activate)
    }

    @Test func accordionLayout_onlyDismissesVisibleAddressFocusWhenActuallyObscured() {
        let obscured = toolbarAccordionTabPresentation(
            tabFrame: CGRect(x: -10, y: 2, width: 68, height: 32),
            viewportWidth: 300
        )
        let incomingCover = ToolbarAccordionTabPresentation(
            offsetX: 0,
            edge: nil,
            collapseProgress: 0,
            overlapProgress: 0.5
        )

        #expect(toolbarShouldDismissObscuredAddressFocus(
            presentation: obscured,
            isCurrentTab: true,
            isAddressEditing: true,
            showsFocusAffordances: true
        ))
        #expect(!toolbarShouldDismissObscuredAddressFocus(
            presentation: obscured,
            isCurrentTab: true,
            isAddressEditing: true,
            showsFocusAffordances: false
        ))
        #expect(!toolbarShouldDismissObscuredAddressFocus(
            presentation: incomingCover,
            isCurrentTab: true,
            isAddressEditing: true,
            showsFocusAffordances: true
        ))
    }

    @Test func largeTabMovingLeft_beforeSmallTabsMidpoint_doesNotReorder() {
        let (viewModel, coordinator, smallTabID, largeTabID) = makeTwoTabFixture()

        // Chrome compares the current moving origin with the two landing
        // origins (x=2 and x=96). At x=50 the current slot is still closer.
        coordinator.updateInPlaceDrag(
            startLocationX: 306,
            locationX: 260,
            activeTabID: largeTabID,
            viewModel: viewModel
        )

        #expect(viewModel.tabs.map(\.id) == [smallTabID, largeTabID])
        coordinator.cancel(activeTabID: largeTabID)
    }

    @Test func largeTabMovingLeft_beyondSmallTabsMidpoint_reordersImmediately() {
        let (viewModel, coordinator, smallTabID, largeTabID) = makeTwoTabFixture()

        // At x=48 the preceding landing slot is closer than x=96.
        coordinator.updateInPlaceDrag(
            startLocationX: 306,
            locationX: 258,
            activeTabID: largeTabID,
            viewModel: viewModel
        )

        #expect(viewModel.tabs.map(\.id) == [largeTabID, smallTabID])
        coordinator.cancel(activeTabID: largeTabID)
    }

    @Test func smallTabMovingRight_waitsUntilTheNextLandingSlotIsCloser() {
        let (viewModel, coordinator, smallTabID, largeTabID) = makeTwoTabFixture()

        // The landing origins are x=2 and x=424, so Chrome's rule keeps the
        // small Tab in place at x=213 (an exact tie favors the earlier slot).
        coordinator.updateInPlaceDrag(
            startLocationX: 48,
            locationX: 259,
            activeTabID: largeTabID,
            viewModel: viewModel
        )

        #expect(viewModel.tabs.map(\.id) == [smallTabID, largeTabID])

        // One point farther makes the next landing slot strictly closer.
        coordinator.updateInPlaceDrag(
            startLocationX: 48,
            locationX: 260,
            activeTabID: largeTabID,
            viewModel: viewModel
        )

        #expect(viewModel.tabs.map(\.id) == [largeTabID, smallTabID])
        coordinator.cancel(activeTabID: largeTabID)
    }

    @Test func movingForegroundFrame_remainsAtDragOriginWhenUnderlyingSlotMoves() {
        let (viewModel, coordinator, smallTabID, largeTabID) = makeTwoTabFixture()

        coordinator.updateInPlaceDrag(
            startLocationX: 306,
            locationX: 257,
            activeTabID: largeTabID,
            viewModel: viewModel
        )

        let frameBeforeSlotMove = coordinator.movingTabFrame(for: largeTabID)
        #expect(frameBeforeSlotMove?.minX == 47)

        // SwiftUI has now moved the transparent slot to the reordered model
        // position. The separately rendered foreground must remain attached
        // to the pointer origin instead of following or recreating that slot.
        coordinator.updateTabFrames([
            largeTabID: CGRect(x: 2, y: 2, width: 420, height: 32),
            smallTabID: CGRect(x: 424, y: 2, width: 92, height: 32),
        ])

        let frameAfterSlotMove = coordinator.movingTabFrame(for: largeTabID)
        #expect(frameAfterSlotMove?.minX == frameBeforeSlotMove?.minX)
        #expect(frameAfterSlotMove?.size == frameBeforeSlotMove?.size)
        coordinator.cancel(activeTabID: largeTabID)
    }

    @Test func chromeRule_doesNotRecalculateFromStalePreMoveSlots() {
        let (viewModel, coordinator, smallTabID, largeTabID) = makeTwoTabFixture()

        coordinator.updateInPlaceDrag(
            startLocationX: 48,
            locationX: 260,
            activeTabID: largeTabID,
            viewModel: viewModel
        )
        #expect(viewModel.tabs.map(\.id) == [largeTabID, smallTabID])

        // The model already moved, but SwiftUI has not reported its new ideal
        // slot geometry yet. Reusing these stale frames would immediately pick
        // the old position and make the two Tabs visibly bounce.
        coordinator.updateInPlaceDrag(
            startLocationX: 48,
            locationX: 261,
            activeTabID: largeTabID,
            viewModel: viewModel
        )
        #expect(viewModel.tabs.map(\.id) == [largeTabID, smallTabID])

        coordinator.updateTabFrames([
            largeTabID: CGRect(x: 2, y: 2, width: 420, height: 32),
            smallTabID: CGRect(x: 424, y: 2, width: 92, height: 32),
        ])
        coordinator.cancel(activeTabID: largeTabID)
    }

    @Test func groupHeaderDrag_movesTheCompleteGroupAtChromesNearestSlot() {
        let fixture = makeGroupDragFixture()

        // The group can land at x=96 (current) or x=190 (after the trailing
        // Tab). At x=142 the current landing position is still closer.
        fixture.coordinator.updateInPlaceDrag(
            startLocationX: 130,
            locationX: 176,
            activeTabID: fixture.trailingTabID,
            viewModel: fixture.viewModel
        )
        #expect(fixture.viewModel.tabs.map(\.id) == [
            fixture.leadingTabID,
            fixture.firstGroupTabID,
            fixture.secondGroupTabID,
            fixture.trailingTabID,
        ])
        #expect(fixture.coordinator.movingGroupID == fixture.groupID)
        #expect(fixture.coordinator.movingTabID == nil)

        // At x=144 the trailing landing position is closer. Chrome moves the
        // header and every member Tab as one block, preserving internal order.
        fixture.coordinator.updateInPlaceDrag(
            startLocationX: 130,
            locationX: 178,
            activeTabID: fixture.trailingTabID,
            viewModel: fixture.viewModel
        )
        #expect(fixture.viewModel.tabs.map(\.id) == [
            fixture.leadingTabID,
            fixture.trailingTabID,
            fixture.firstGroupTabID,
            fixture.secondGroupTabID,
        ])
        #expect(fixture.viewModel.tabs.suffix(2).allSatisfy { $0.groupId == fixture.groupID })
        #expect(fixture.coordinator.movingGroupFrame(for: fixture.groupID)?.minX == 144)
        fixture.coordinator.cancel(activeTabID: fixture.trailingTabID)
    }

    private func makeTwoTabFixture() -> (
        BrowserViewModel,
        ToolbarTabInteractionCoordinator,
        UUID,
        UUID
    ) {
        let viewModel = BrowserViewModel()
        let smallTabID = viewModel.tabs[0].id
        let largeTabID = viewModel.addNewTab(focusAddressBar: false).id
        let coordinator = ToolbarTabInteractionCoordinator()

        coordinator.updateTabFrames([
            smallTabID: CGRect(x: 2, y: 2, width: 92, height: 32),
            largeTabID: CGRect(x: 96, y: 2, width: 420, height: 32),
        ])
        coordinator.updateTabStripWidth(600)

        return (viewModel, coordinator, smallTabID, largeTabID)
    }

    private func twoTabChromeCandidates() -> [ToolbarTabChromeInsertionCandidate] {
        [
            ToolbarTabChromeInsertionCandidate(preRemovalIndex: 0, idealOriginXs: [2]),
            ToolbarTabChromeInsertionCandidate(preRemovalIndex: 1, idealOriginXs: [96]),
        ]
    }

    private func makeGroupDragFixture() -> (
        viewModel: BrowserViewModel,
        coordinator: ToolbarTabInteractionCoordinator,
        groupID: UUID,
        leadingTabID: UUID,
        firstGroupTabID: UUID,
        secondGroupTabID: UUID,
        trailingTabID: UUID
    ) {
        let viewModel = BrowserViewModel()
        let leadingTabID = viewModel.tabs[0].id
        let firstGroupTabID = viewModel.addNewTab(focusAddressBar: false).id
        let secondGroupTabID = viewModel.addNewTab(focusAddressBar: false).id
        let trailingTabID = viewModel.addNewTab(focusAddressBar: false).id
        let group = viewModel.createGroup(name: "Group", color: .red)
        viewModel.addTabToGroup(tabId: firstGroupTabID, groupId: group.id)
        viewModel.addTabToGroup(tabId: secondGroupTabID, groupId: group.id)

        let coordinator = ToolbarTabInteractionCoordinator()
        coordinator.updateTabFrames([
            leadingTabID: CGRect(x: 2, y: 2, width: 92, height: 32),
            firstGroupTabID: CGRect(x: 180, y: 2, width: 92, height: 32),
            secondGroupTabID: CGRect(x: 274, y: 2, width: 92, height: 32),
            trailingTabID: CGRect(x: 368, y: 2, width: 92, height: 32),
        ])
        coordinator.updateGroupFrames([
            group.id: CGRect(x: 96, y: 2, width: 270, height: 32),
        ])
        coordinator.updateGroupHeaderFrames([
            group.id: CGRect(x: 96, y: 2, width: 82, height: 30),
        ])
        coordinator.updateTabStripWidth(700)

        return (
            viewModel,
            coordinator,
            group.id,
            leadingTabID,
            firstGroupTabID,
            secondGroupTabID,
            trailingTabID
        )
    }
}
