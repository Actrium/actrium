import Combine
import Foundation
import SwiftUI

private enum ToolbarInPlaceDragSubject: Equatable {
    case tab(UUID)
    case group(UUID)
}

private struct ToolbarInPlaceDragState: Equatable {
    let subject: ToolbarInPlaceDragSubject
    let grabOffsetX: CGFloat
    var originX: CGFloat
    var lastReorderPointerX: CGFloat?
}

enum ToolbarTabInteractionPhase: Equatable {
    case inactive, active, addressEditing, draggingInactive, draggingActive
}

enum ToolbarTabInteractionInput {
    case activate, deactivate, beginAddressEditing, endAddressEditing
    case beginDrag, finishDragInactive, finishDragActive, cancel
}

struct ToolbarTabInteractionTransitionInfo {
    let phase: ToolbarTabInteractionPhase
    let accepted: Bool
    var permitsTabDrag: Bool { phase != .addressEditing }
    var suppressesWindowDrag: Bool { true }
}

/// The August client transition table, kept local to toolbar chrome so the
/// current Rust core and its unrelated persistence contracts stay intact.
func toolbarTabInteractionTransition(
    phase: ToolbarTabInteractionPhase,
    input: ToolbarTabInteractionInput
) -> ToolbarTabInteractionTransitionInfo {
    let next: ToolbarTabInteractionPhase?
    switch (phase, input) {
    case (.inactive, .activate): next = .active
    case (.inactive, .deactivate), (.inactive, .cancel): next = .inactive
    case (.inactive, .beginDrag): next = .draggingInactive
    case (.active, .activate), (.active, .endAddressEditing), (.active, .cancel): next = .active
    case (.active, .deactivate): next = .inactive
    case (.active, .beginAddressEditing): next = .addressEditing
    case (.active, .beginDrag): next = .draggingActive
    case (.addressEditing, .activate), (.addressEditing, .beginAddressEditing): next = .addressEditing
    case (.addressEditing, .deactivate): next = .inactive
    case (.addressEditing, .endAddressEditing), (.addressEditing, .cancel): next = .active
    case (.draggingInactive, .finishDragInactive), (.draggingInactive, .cancel): next = .inactive
    case (.draggingInactive, .finishDragActive): next = .active
    case (.draggingActive, .finishDragInactive): next = .inactive
    case (.draggingActive, .finishDragActive), (.draggingActive, .cancel): next = .active
    default: next = nil
    }
    return ToolbarTabInteractionTransitionInfo(
        phase: next ?? phase,
        accepted: next != nil
    )
}

struct ToolbarTabChromeInsertionCandidate: Equatable {
    let preRemovalIndex: Int
    let idealOriginXs: [CGFloat]
}

/// Port of Chromium TabStrip::CalculateInsertionIndex's selection rule.
///
/// Every legal insertion point contributes the horizontal origin where the
/// moving Tab would settle if released. The closest ideal origin wins. As in
/// Chromium, ties keep the earlier candidate because candidates are evaluated
/// from left to right, and the returned index is expressed after removal.
func toolbarTabChromeInsertionIndex(
    draggedOriginX: CGFloat,
    sourceIndex: Int,
    draggedTabCount: Int = 1,
    candidates: [ToolbarTabChromeInsertionCandidate]
) -> Int? {
    var minimumDistance = CGFloat.greatestFiniteMagnitude
    var minimumDistanceIndex: Int?

    for candidate in candidates.sorted(by: { $0.preRemovalIndex < $1.preRemovalIndex }) {
        guard let distance = candidate.idealOriginXs
            .map({ abs(draggedOriginX - $0) })
            .min() else { continue }
        if distance < minimumDistance {
            minimumDistance = distance
            minimumDistanceIndex = candidate.preRemovalIndex
        }
    }

    guard var insertionIndex = minimumDistanceIndex else { return nil }
    if insertionIndex > sourceIndex {
        insertionIndex -= draggedTabCount
    }
    return insertionIndex
}

/// A group at either logical end needs an outside drop slot even when its
/// visible tabs fill the strip. The pointer may leave the strip during a drag;
/// a small in-strip threshold makes the slot reachable at the window edge.
func toolbarEdgeUngroupInsertionIndex(
    pointerX: CGFloat,
    viewportWidth: CGFloat,
    sourceGroupID: UUID?,
    tabGroupIDs: [UUID?],
    edgeThreshold: CGFloat = 8
) -> Int? {
    guard let sourceGroupID, !tabGroupIDs.isEmpty, viewportWidth > 0 else {
        return nil
    }
    if tabGroupIDs[0] == sourceGroupID, pointerX <= edgeThreshold {
        return 0
    }
    if tabGroupIDs[tabGroupIDs.count - 1] == sourceGroupID,
       pointerX >= viewportWidth - edgeThreshold {
        return tabGroupIDs.count - 1
    }
    return nil
}

func toolbarDragCanReorder(
    pointerX: CGFloat,
    lastReorderPointerX: CGFloat?,
    minimumTravel: CGFloat = 12
) -> Bool {
    // A stationary pointer must not reverse a move while SwiftUI catches up
    // with the changed group header and tab slots.
    guard let lastReorderPointerX else { return true }
    return abs(pointerX - lastReorderPointerX) >= minimumTravel
}

/// Owns the one exclusive Tab interaction that can be in progress in the compact toolbar.
/// Its transition table is local to this view layer.
@MainActor
final class ToolbarTabInteractionCoordinator: ObservableObject {
    @Published private(set) var phase: ToolbarTabInteractionPhase = .active
    @Published private(set) var interactionTabID: UUID?
    @Published private var inPlaceDragState: ToolbarInPlaceDragState?
    @Published private var tabFrames: [UUID: CGRect] = [:]
    @Published private var groupFrames: [UUID: CGRect] = [:]
    @Published private var groupHeaderFrames: [UUID: CGRect] = [:]
    @Published private var tabStripWidth: CGFloat = 0

    private weak var reorderViewModel: BrowserViewModel?
    private var pendingVisualTabOrder: [UUID]?
    private var pendingGroupFrameChange: (groupID: UUID, previousFrame: CGRect)?

    var isAddressEditing: Bool {
        phase == .addressEditing
    }

    var isDragging: Bool {
        phase == .draggingInactive || phase == .draggingActive
    }

    var movingTabID: UUID? {
        guard case let .tab(tabID) = inPlaceDragState?.subject else { return nil }
        return tabID
    }

    var movingGroupID: UUID? {
        guard case let .group(groupID) = inPlaceDragState?.subject else { return nil }
        return groupID
    }

    func isMovingTab(_ tabID: UUID) -> Bool {
        movingTabID == tabID
    }

    func isMovingGroup(_ groupID: UUID) -> Bool {
        movingGroupID == groupID
    }

    /// The moving Tab is rendered once by the Tabs root overlay. Its frame is
    /// expressed in the same coordinate space as the strip, so crossing a
    /// nested Tabs Group never recreates the foreground view.
    func movingTabFrame(for tabID: UUID) -> CGRect? {
        guard let drag = inPlaceDragState,
              drag.subject == .tab(tabID),
              let slotFrame = tabFrames[tabID] else { return nil }
        return CGRect(
            x: drag.originX,
            y: slotFrame.minY,
            width: slotFrame.width,
            height: slotFrame.height
        )
    }

    /// Group-header dragging uses the union of the header and all visible Tabs,
    /// exactly like Chromium's dragged group bounds.
    func movingGroupFrame(for groupID: UUID) -> CGRect? {
        guard let drag = inPlaceDragState,
              drag.subject == .group(groupID),
              let slotFrame = groupFrames[groupID] else { return nil }
        return CGRect(
            x: drag.originX,
            y: slotFrame.minY,
            width: slotFrame.width,
            height: slotFrame.height
        )
    }

    func updateTabFrames(_ frames: [UUID: CGRect]) {
        guard tabFrames != frames else { return }
        tabFrames = frames

        // Chromium updates its model and ideal bounds synchronously. SwiftUI
        // reports the newly laid-out slots on the next render pass, so do not
        // calculate another insertion from stale pre-move geometry.
        if let pendingVisualTabOrder {
            let framedIDs = Set(frames.keys)
            let expectedVisibleOrder = pendingVisualTabOrder.filter { framedIDs.contains($0) }
            let actualVisibleOrder = frames
                .sorted { $0.value.minX < $1.value.minX }
                .map(\.key)
            if actualVisibleOrder == expectedVisibleOrder {
                self.pendingVisualTabOrder = nil
            }
        }
    }

    func updateGroupFrames(_ frames: [UUID: CGRect]) {
        guard groupFrames != frames else { return }
        groupFrames = frames

        if let pendingGroupFrameChange {
            if let frame = frames[pendingGroupFrameChange.groupID] {
                if abs(frame.minX - pendingGroupFrameChange.previousFrame.minX) > 0.5
                    || abs(frame.width - pendingGroupFrameChange.previousFrame.width) > 0.5 {
                    self.pendingGroupFrameChange = nil
                }
            } else {
                self.pendingGroupFrameChange = nil
            }
        }
    }

    func updateGroupHeaderFrames(_ frames: [UUID: CGRect]) {
        guard groupHeaderFrames != frames else { return }
        groupHeaderFrames = frames
    }

    func updateTabStripWidth(_ width: CGFloat) {
        guard tabStripWidth != width else { return }
        tabStripWidth = width
    }

    func revealAlignment(for tabID: UUID) -> ToolbarTabRevealAlignment? {
        guard let frame = tabFrames[tabID] else { return nil }
        return toolbarTabRevealAlignment(
            tabFrame: frame,
            viewportWidth: tabStripWidth
        )
    }

    func phase(for tabID: UUID, activeTabID: UUID?) -> ToolbarTabInteractionPhase {
        if interactionTabID == tabID {
            switch phase {
            case .addressEditing:
                return tabID == activeTabID ? .addressEditing : .inactive
            case .draggingInactive, .draggingActive:
                return tabID == activeTabID ? .draggingActive : .draggingInactive
            case .inactive, .active:
                break
            }
        }
        return tabID == activeTabID ? .active : .inactive
    }

    @discardableResult
    func beginAddressEditing(tabID: UUID, activeTabID: UUID?) -> Bool {
        guard tabID == activeTabID,
              !isDragging,
              interactionTabID == nil || interactionTabID == tabID else { return false }
        let initialPhase = phase(for: tabID, activeTabID: activeTabID)
        return apply(
            from: initialPhase,
            input: .beginAddressEditing,
            tabID: tabID
        ).accepted
    }

    func endAddressEditing(activeTabID: UUID?) {
        guard phase == .addressEditing else { return }
        let result = apply(input: .endAddressEditing)
        if result.accepted {
            clearExclusiveInteraction(activeTabID: activeTabID)
        }
    }

    @discardableResult
    func beginDrag(tabID: UUID, activeTabID: UUID?) -> Bool {
        guard interactionTabID == nil || interactionTabID == tabID else { return false }
        let initialPhase = phase(for: tabID, activeTabID: activeTabID)
        let result = apply(from: initialPhase, input: .beginDrag, tabID: tabID)
        return result.accepted && result.permitsTabDrag && result.suppressesWindowDrag
    }

    func finishDrag(activeTabID: UUID?) {
        let viewModel = reorderViewModel
        reorderViewModel = nil
        inPlaceDragState = nil
        pendingVisualTabOrder = nil
        pendingGroupFrameChange = nil

        guard isDragging else { return }
        let input: ToolbarTabInteractionInput = interactionTabID == activeTabID
            ? .finishDragActive
            : .finishDragInactive
        let result = apply(input: input)
        if result.accepted {
            viewModel?.commitInteractiveTabReorder()
            clearExclusiveInteraction(activeTabID: activeTabID)
        }
    }

    /// Drives a Tab entirely inside the toolbar's coordinate space. No
    /// NSDraggingSession is created, so AppKit has no drag image to lift out of
    /// the strip. The real Tab follows the pointer while sibling slots reorder.
    func updateInPlaceDrag(
        startLocationX: CGFloat,
        locationX: CGFloat,
        activeTabID: UUID?,
        viewModel: BrowserViewModel
    ) {
        let subject: ToolbarInPlaceDragSubject
        if let existingSubject = inPlaceDragState?.subject {
            subject = existingSubject
        } else if let sourceGroup = groupHeaderFrames
            .filter({ $0.value.minX <= startLocationX && startLocationX <= $0.value.maxX })
            .min(by: { $0.value.width < $1.value.width }) {
            subject = .group(sourceGroup.key)
        } else if let source = tabFrames
            .filter({ $0.value.minX <= startLocationX && startLocationX <= $0.value.maxX })
            .min(by: { $0.value.width < $1.value.width }) {
            subject = .tab(source.key)
        } else {
            return
        }

        let frame: CGRect
        let interactionTabID: UUID
        switch subject {
        case let .tab(tabID):
            guard let tabFrame = tabFrames[tabID] else { return }
            frame = tabFrame
            interactionTabID = tabID
        case let .group(groupID):
            guard let groupFrame = groupFrames[groupID] else { return }
            let groupTabs = viewModel.tabs.filter { $0.groupId == groupID }
            guard let representativeTab = groupTabs.first(where: { $0.id == activeTabID })
                ?? groupTabs.first else { return }
            frame = groupFrame
            interactionTabID = representativeTab.id
        }

        if inPlaceDragState == nil {
            guard beginDrag(tabID: interactionTabID, activeTabID: activeTabID) else { return }
            reorderViewModel = viewModel
            inPlaceDragState = ToolbarInPlaceDragState(
                subject: subject,
                grabOffsetX: startLocationX - frame.minX,
                originX: frame.minX,
                lastReorderPointerX: nil
            )
        }

        guard var drag = inPlaceDragState, drag.subject == subject else { return }
        let leadingInset: CGFloat = 2
        let trailingLimit = max(leadingInset, tabStripWidth - frame.width - leadingInset)
        let nextOriginX = min(
            max(locationX - drag.grabOffsetX, leadingInset),
            trailingLimit
        )
        if nextOriginX != drag.originX {
            drag.originX = nextOriginX
            inPlaceDragState = drag
        }

        let draggedFrame = CGRect(
            x: nextOriginX,
            y: frame.minY,
            width: frame.width,
            height: frame.height
        )
        switch subject {
        case let .tab(tabID):
            reorderTabIfNeeded(
                tabID: tabID,
                pointerX: locationX,
                draggedFrame: draggedFrame,
                viewModel: viewModel
            )
        case let .group(groupID):
            reorderGroupIfNeeded(
                groupID: groupID,
                draggedFrame: draggedFrame,
                viewModel: viewModel
            )
        }
    }

    /// Completes the exclusive drag when AppKit ends the originating pointer
    /// sequence, including drops outside every toolbar target.
    func pointerSequenceEnded(activeTabID: UUID?) {
        finishDrag(activeTabID: activeTabID)
    }

    func activeTabChanged(to activeTabID: UUID?) {
        guard phase == .addressEditing, interactionTabID != activeTabID else { return }
        let result = apply(input: .deactivate)
        if result.accepted {
            clearExclusiveInteraction(activeTabID: activeTabID)
        }
    }

    func cancel(activeTabID: UUID?) {
        let viewModel = reorderViewModel
        reorderViewModel = nil
        inPlaceDragState = nil
        pendingVisualTabOrder = nil
        pendingGroupFrameChange = nil

        guard interactionTabID != nil else { return }
        _ = apply(input: .cancel)
        viewModel?.commitInteractiveTabReorder()
        clearExclusiveInteraction(activeTabID: activeTabID)
    }

    private func reorderTabIfNeeded(
        tabID: UUID,
        pointerX: CGFloat,
        draggedFrame: CGRect,
        viewModel: BrowserViewModel
    ) {
        guard pendingVisualTabOrder == nil,
              pendingGroupFrameChange == nil,
              toolbarDragCanReorder(
                  pointerX: pointerX,
                  lastReorderPointerX: inPlaceDragState?.lastReorderPointerX
              ) else { return }
        guard let sourceIndex = viewModel.tabs.firstIndex(where: { $0.id == tabID }) else {
            return
        }
        if let edgeIndex = toolbarEdgeUngroupInsertionIndex(
            pointerX: pointerX,
            viewportWidth: tabStripWidth,
            sourceGroupID: viewModel.tabs[sourceIndex].groupId,
            tabGroupIDs: viewModel.tabs.map(\.groupId)
        ) {
            performInteractiveMove(
                tabID: tabID,
                targetIndex: edgeIndex,
                groupID: nil,
                pointerX: pointerX,
                viewModel: viewModel
            )
            return
        }
        let candidates = chromeInsertionCandidates(
            tabs: viewModel.tabs,
            sourceIndex: sourceIndex,
            draggedFrame: draggedFrame,
            groups: viewModel.tabGroups,
            draggedTabCount: 1,
            draggedGroupID: nil
        )
        guard let targetIndex = toolbarTabChromeInsertionIndex(
            draggedOriginX: draggedFrame.minX,
            sourceIndex: sourceIndex,
            candidates: candidates
        ) else { return }

        let groupID = chromeTargetGroupID(
            movingTabID: tabID,
            targetIndex: targetIndex,
            pointerX: pointerX,
            viewModel: viewModel
        )
        performInteractiveMove(
            tabID: tabID,
            targetIndex: targetIndex,
            groupID: groupID,
            pointerX: pointerX,
            viewModel: viewModel
        )
    }

    /// Chromium treats a header drag as one block containing the header and
    /// every Tab in that group. The block uses the same nearest-ideal-slot
    /// calculation as a Tab, but slots inside any group are illegal.
    private func reorderGroupIfNeeded(
        groupID: UUID,
        draggedFrame: CGRect,
        viewModel: BrowserViewModel
    ) {
        guard pendingVisualTabOrder == nil, pendingGroupFrameChange == nil else { return }
        let groupTabs = viewModel.tabs.filter { $0.groupId == groupID }
        guard let firstGroupTab = groupTabs.first,
              let sourceIndex = viewModel.tabs.firstIndex(where: { $0.id == firstGroupTab.id }) else {
            return
        }

        let candidates = chromeInsertionCandidates(
            tabs: viewModel.tabs,
            sourceIndex: sourceIndex,
            draggedFrame: draggedFrame,
            groups: viewModel.tabGroups,
            draggedTabCount: groupTabs.count,
            draggedGroupID: groupID
        )
        guard let targetIndex = toolbarTabChromeInsertionIndex(
            draggedOriginX: draggedFrame.minX,
            sourceIndex: sourceIndex,
            draggedTabCount: groupTabs.count,
            candidates: candidates
        ) else { return }

        performInteractiveGroupMove(
            groupID: groupID,
            sourceIndex: sourceIndex,
            targetIndex: targetIndex,
            viewModel: viewModel
        )
    }

    /// Builds Chromium-style legal insertion slots from Actrium's live ideal
    /// geometry. At the leading edge of an expanded group, both sides of the
    /// group header are candidates, matching Chromium's group-header adjustment.
    private func chromeInsertionCandidates(
        tabs: [Tab],
        sourceIndex: Int,
        draggedFrame: CGRect,
        groups: [TabGroup],
        draggedTabCount: Int,
        draggedGroupID: UUID?
    ) -> [ToolbarTabChromeInsertionCandidate] {
        let spacing: CGFloat = 2
        let collapsedGroupIDs = Set(groups.filter(\.isCollapsed).map(\.id))
        let leadingOriginX = (Array(tabFrames.values.map(\.minX))
            + Array(groupFrames.values.map(\.minX))).min() ?? spacing

        return (0...tabs.count).compactMap { candidateIndex in
            // Chromium rejects the slot immediately after the dragged range;
            // after removal it would be the same logical position.
            if candidateIndex > sourceIndex &&
                candidateIndex <= sourceIndex + draggedTabCount {
                return nil
            }

            let leftTab = candidateIndex > 0 ? tabs[candidateIndex - 1] : nil
            let rightTab = candidateIndex < tabs.count ? tabs[candidateIndex] : nil
            let leftGroupID = leftTab?.groupId
            let rightGroupID = rightTab?.groupId

            // Chrome does not allow a Tab to be inserted inside a collapsed
            // group. The boundaries on either side remain legal candidates.
            if let sharedGroupID = leftGroupID, sharedGroupID == rightGroupID {
                if draggedGroupID != nil || collapsedGroupIDs.contains(sharedGroupID) {
                    return nil
                }
            }

            var idealOrigins: [CGFloat] = []
            if let rightGroupID,
               rightGroupID != leftGroupID,
               let groupFrame = groupFrames[rightGroupID] {
                // Left of the header is always legal. If expanded, Chromium
                // also evaluates the position just to the header's right.
                idealOrigins.append(groupFrame.minX)
                if draggedGroupID == nil,
                   !collapsedGroupIDs.contains(rightGroupID),
                   let rightTab,
                   let rightFrame = tabFrames[rightTab.id] {
                    idealOrigins.append(rightFrame.minX)
                }
            } else if let rightTab, let rightFrame = tabFrames[rightTab.id] {
                idealOrigins.append(rightFrame.minX)
            } else if let leftGroupID,
                      collapsedGroupIDs.contains(leftGroupID),
                      let groupFrame = groupFrames[leftGroupID] {
                idealOrigins.append(groupFrame.maxX + spacing)
            } else if let leftTab, let leftFrame = tabFrames[leftTab.id] {
                idealOrigins.append(leftFrame.maxX + spacing)
            } else if candidateIndex == 0 {
                idealOrigins.append(leadingOriginX)
            }

            guard !idealOrigins.isEmpty else { return nil }

            // Chromium calculates from the current ideal bounds, then removes
            // the moving Tab's footprint for candidates to its right.
            if candidateIndex > sourceIndex + draggedTabCount {
                let footprint = draggedFrame.width + spacing
                idealOrigins = idealOrigins.map { $0 - footprint }
            }

            return ToolbarTabChromeInsertionCandidate(
                preRemovalIndex: candidateIndex,
                idealOriginXs: idealOrigins
            )
        }
    }

    /// Group membership is a separate decision in Chromium as well. An
    /// expanded group under the pointer wins; otherwise only a slot strictly
    /// inside one group inherits that group. Collapsed groups never accept a
    /// dragged Tab.
    private func chromeTargetGroupID(
        movingTabID: UUID,
        targetIndex: Int,
        pointerX: CGFloat,
        viewModel: BrowserViewModel
    ) -> UUID? {
        if let hoveredGroup = groupFrames
            .filter({ $0.value.minX <= pointerX && pointerX <= $0.value.maxX })
            .min(by: { abs($0.value.midX - pointerX) < abs($1.value.midX - pointerX) }),
           viewModel.tabGroups.first(where: { $0.id == hoveredGroup.key })?.isCollapsed == false {
            return hoveredGroup.key
        }

        var remainingTabs = viewModel.tabs
        remainingTabs.removeAll { $0.id == movingTabID }
        let boundedIndex = min(max(0, targetIndex), remainingTabs.count)
        let leftGroupID = boundedIndex > 0 ? remainingTabs[boundedIndex - 1].groupId : nil
        let rightGroupID = boundedIndex < remainingTabs.count
            ? remainingTabs[boundedIndex].groupId
            : nil
        if let leftGroupID,
           leftGroupID == rightGroupID,
           viewModel.tabGroups.first(where: { $0.id == leftGroupID })?.isCollapsed == false {
            return leftGroupID
        }
        return nil
    }

    private func performInteractiveMove(
        tabID: UUID,
        targetIndex: Int,
        groupID: UUID?,
        pointerX: CGFloat,
        viewModel: BrowserViewModel
    ) {
        guard let fromIndex = viewModel.tabs.firstIndex(where: { $0.id == tabID }) else { return }
        let boundedTargetIndex = min(max(0, targetIndex), max(0, viewModel.tabs.count - 1))

        let currentGroupID = viewModel.tabs[fromIndex].groupId
        guard boundedTargetIndex != fromIndex || currentGroupID != groupID else { return }

        let previousVisualOrder = visibleModelTabOrder(viewModel)
        let changedGroupID = groupID != currentGroupID ? (groupID ?? currentGroupID) : nil
        let previousGroupFrame = changedGroupID.flatMap { groupFrames[$0] }
        withAnimation(ToolbarTabMotion.slide) {
            viewModel.moveTabInteractively(
                tabId: tabID,
                toIndex: boundedTargetIndex,
                groupId: groupID
            )
        }
        let nextVisualOrder = visibleModelTabOrder(viewModel)
        if nextVisualOrder != previousVisualOrder {
            pendingVisualTabOrder = nextVisualOrder
        }
        if let changedGroupID, let previousGroupFrame {
            pendingGroupFrameChange = (changedGroupID, previousGroupFrame)
        }
        inPlaceDragState?.lastReorderPointerX = pointerX
    }

    private func performInteractiveGroupMove(
        groupID: UUID,
        sourceIndex: Int,
        targetIndex: Int,
        viewModel: BrowserViewModel
    ) {
        let remainingTabCount = viewModel.tabs.filter { $0.groupId != groupID }.count
        let boundedTargetIndex = min(max(0, targetIndex), remainingTabCount)
        guard boundedTargetIndex != sourceIndex else { return }

        let previousVisualOrder = visibleModelTabOrder(viewModel)
        let previousGroupFrame = groupFrames[groupID]
        withAnimation(ToolbarTabMotion.slide) {
            viewModel.moveGroupInteractively(groupId: groupID, toIndex: boundedTargetIndex)
        }
        let nextVisualOrder = visibleModelTabOrder(viewModel)
        if nextVisualOrder != previousVisualOrder {
            pendingVisualTabOrder = nextVisualOrder
        }
        if let previousGroupFrame {
            pendingGroupFrameChange = (groupID, previousGroupFrame)
        }
    }

    private func visibleModelTabOrder(_ viewModel: BrowserViewModel) -> [UUID] {
        let collapsedGroupIDs = Set(viewModel.tabGroups.filter(\.isCollapsed).map(\.id))
        return viewModel.tabsOrganizedByGroups().flatMap { section -> [UUID] in
            if let groupID = section.group?.id, collapsedGroupIDs.contains(groupID) {
                return []
            }
            return section.tabs.map(\.id)
        }
    }

    @discardableResult
    private func apply(
        from initialPhase: ToolbarTabInteractionPhase? = nil,
        input: ToolbarTabInteractionInput,
        tabID: UUID? = nil
    ) -> ToolbarTabInteractionTransitionInfo {
        let result = toolbarTabInteractionTransition(
            phase: initialPhase ?? phase,
            input: input
        )
        guard result.accepted else { return result }

        phase = result.phase
        if let tabID {
            interactionTabID = tabID
        }
        return result
    }

    private func clearExclusiveInteraction(activeTabID: UUID?) {
        interactionTabID = nil
        phase = activeTabID == nil ? .inactive : .active
    }
}
