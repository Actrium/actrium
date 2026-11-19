import Foundation
import Testing
@testable import Actrium

@MainActor
struct TabWindowTransferFocusTests {
    @Test func movingAFocusedGroupRetargetsBothRealViewModels() throws {
        let group = TabGroup(name: "Apps")
        let first = Tab(title: "First", groupId: group.id)
        let remaining = Tab(title: "Remaining")
        let selected = Tab(title: "Selected", groupId: group.id)
        let source = window(tabs: [first, remaining, selected], selected: selected)
        source.tabGroups = [group]
        let destination = BrowserViewModel(saveBrowserSession: { _, _ in })
        #expect(TabGroupWindowTransfer(source: source, group: group).move(to: destination))
        #expect(source.tabs.map(\.id) == [remaining.id])
        #expect(source.currentTabId == remaining.id)
        #expect(source.actriumMode == .focus(tabId: remaining.id))
        #expect(source.tabSlotPositions[remaining.id] == 0)
        #expect(source.tabGroups.isEmpty)
        let destinationID = try #require(destination.currentTabId)
        #expect(destination.actriumMode == .focus(tabId: destinationID))
    }

    @Test func movingAFocusedTabSelectsItsNeighborInTheRealSource() throws {
        let first = Tab(title: "First")
        let selected = Tab(title: "Selected")
        let last = Tab(title: "Last")
        let source = window(tabs: [first, selected, last], selected: selected)
        let destination = BrowserViewModel(saveBrowserSession: { _, _ in })
        #expect(TabWindowTransfer.moveTab(selected.id, from: source, to: destination, at: 0, groupId: nil))
        #expect(source.currentTabId == first.id)
        #expect(source.actriumMode == .focus(tabId: first.id))
        #expect(destination.currentTab?.title == selected.title)
        let destinationID = try #require(destination.currentTabId)
        #expect(destination.actriumMode == .focus(tabId: destinationID))
    }

    @Test func movingTheLastTabClearsFocusWithoutClosingTheApplication() {
        let selected = Tab(title: "Selected")
        let source = window(tabs: [selected], selected: selected)
        let destination = BrowserViewModel(saveBrowserSession: { _, _ in })
        #expect(TabWindowTransfer.moveTab(selected.id, from: source, to: destination, at: 0, groupId: nil))
        #expect(source.tabs.isEmpty)
        #expect(source.currentTabId == nil)
        #expect(source.actriumMode == .overview)
        #expect(destination.currentTab?.title == selected.title)
    }

    @Test func transfersPreserveOverviewAndAnUnmovedSelection() {
        let selected = Tab(title: "Selected")
        let other = Tab(title: "Other")
        let source = window(tabs: [selected, other], selected: selected)
        let destination = BrowserViewModel(saveBrowserSession: { _, _ in })
        destination.actriumMode = .overview
        #expect(TabWindowTransfer.moveTab(other.id, from: source, to: destination, at: 0, groupId: nil))
        #expect(source.currentTabId == selected.id)
        #expect(source.actriumMode == .focus(tabId: selected.id))
        #expect(destination.actriumMode == .overview)
        source.actriumMode = .overview
        #expect(TabWindowTransfer.moveTab(selected.id, from: source, to: destination, at: 0, groupId: nil))
        #expect(source.actriumMode == .overview)
    }

    @Test func movingTheCurrentTabPreservesOverviewWithOtherTabsRemaining() {
        let first = Tab(title: "First")
        let selected = Tab(title: "Selected")
        let source = window(tabs: [first, selected], selected: selected)
        source.actriumMode = .overview
        let destination = BrowserViewModel(saveBrowserSession: { _, _ in })
        destination.actriumMode = .overview
        #expect(TabWindowTransfer.moveTab(selected.id, from: source, to: destination, at: 0, groupId: nil))
        #expect(source.currentTabId == first.id)
        #expect(source.actriumMode == .overview)
        #expect(destination.actriumMode == .overview)
    }

    @Test func removingTheDisplayedTabRetargetsToTheExistingSelection() {
        let displayed = Tab(title: "Displayed")
        let selected = Tab(title: "Selected")
        let source = window(tabs: [displayed, selected], selected: selected)
        source.actriumMode = .focus(tabId: displayed.id)
        #expect(TabWindowTransfer.moveTab(
            displayed.id, from: source, to: BrowserViewModel(saveBrowserSession: { _, _ in }),
            at: 0, groupId: nil
        ))
        #expect(source.currentTabId == selected.id)
        #expect(source.actriumMode == .focus(tabId: selected.id))
    }

    @Test func transferRemovalCompactsReorderedGridSlotsBeforeAnotherTabIsAdded() {
        let first = Tab(title: "First")
        let selected = Tab(title: "Selected")
        let last = Tab(title: "Last")
        let source = window(tabs: [first, selected, last], selected: selected)
        source.tabSlotPositions = [first.id: 2, selected.id: 0, last.id: 1]
        #expect(TabWindowTransfer.moveTab(
            selected.id, from: source, to: BrowserViewModel(saveBrowserSession: { _, _ in }),
            at: 0, groupId: nil
        ))
        #expect(source.tabSlotPositions[first.id] == 1)
        #expect(source.tabSlotPositions[last.id] == 0)
        _ = source.addNewTab(focusAddressBar: false)
        #expect(source.tabSlotPositions.values.sorted() == Array(0..<source.tabs.count))
    }

    private func window(tabs: [Tab], selected: Tab) -> BrowserViewModel {
        let viewModel = BrowserViewModel(saveBrowserSession: { _, _ in })
        viewModel.tabs = tabs
        viewModel.tabSlotPositions = Dictionary(uniqueKeysWithValues: tabs.enumerated().map { ($0.element.id, $0.offset) })
        viewModel.currentTabId = selected.id
        viewModel.actriumMode = .focus(tabId: selected.id)
        return viewModel
    }
}
