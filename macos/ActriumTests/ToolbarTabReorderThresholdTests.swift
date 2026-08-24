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
