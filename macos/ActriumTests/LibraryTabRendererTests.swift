import ActriumIPC
import Combine
import Foundation
import Testing
@testable import Actrium

@MainActor
struct LibraryTabRendererTests {
    @Test func systemTabsRenderAndRefreshTheirOwnSnapshots() async throws {
        let pool = RendererProcessPool.shared
        let historyTabID = UUID()
        let bookmarksTabID = UUID()
        let visit = HistoryItem(title: "Test visit", appURL: URL(string: "actro://publisher.develenv.com/counterapp@0.1.0"))
        let bookmark = Bookmark(title: "Test bookmark", appURL: URL(string: "https://example.com/docs/start?tab=one"))
        var trees: [UUID: Actrium_Ipc_AccessibilityTree] = [:]
        let updates = NotificationCenter.default.publisher(for: .rendererAccessibilityUpdated).sink { notification in
            guard let id = notification.userInfo?["tabId"] as? UUID,
                  let tree = notification.userInfo?["tree"] as? Actrium_Ipc_AccessibilityTree else { return }
            trees[id] = tree
        }
        var removedVisitID: String?
        let removals = NotificationCenter.default.publisher(for: XPCNotificationBridge.deleteHistoryItem).sink { notification in
            guard notification.userInfo?["tabId"] as? UUID == historyTabID else { return }
            removedVisitID = notification.userInfo?["itemId"] as? String
        }
        func hasLabel(_ label: String, in tabID: UUID) -> Bool {
            trees[tabID]?.nodes.contains(where: { $0.label == label }) == true
        }
        defer {
            updates.cancel()
            removals.cancel()
            pool.releaseRenderer(for: historyTabID)
            pool.releaseRenderer(for: bookmarksTabID)
        }

        let historyRenderer = try pool.allocateRenderer(for: historyTabID, fingerprint: "system.history")
        let bookmarkRenderer = try pool.allocateRenderer(for: bookmarksTabID, fingerprint: "system.bookmarks")
        #expect(historyRenderer == bookmarkRenderer)
        try await render(.history, tabID: historyTabID, history: [visit])
        try await render(.bookmarks, tabID: bookmarksTabID, bookmarks: [bookmark])
        try await waitForUpdate(historyTabID) { hasLabel(visit.title, in: historyTabID) }
        try await waitForUpdate(bookmarksTabID) { hasLabel(bookmark.title, in: bookmarksTabID) }
        let visitLink = try #require(trees[historyTabID]?.nodes.first { $0.label == visit.title })
        let bookmarkLink = try #require(trees[bookmarksTabID]?.nodes.first { $0.label == bookmark.title })
        #expect(visitLink.help == visit.appURL?.absoluteString)
        #expect(bookmarkLink.help == bookmark.appURL?.absoluteString)
        #expect(!hasLabel(bookmark.title, in: historyTabID))
        #expect(!hasLabel(visit.title, in: bookmarksTabID))
        try await verifyOpenAction(visitLink, in: historyTabID, event: XPCNotificationBridge.openHistoryItem,
                                   idKey: "itemId", itemID: visit.id)
        try await verifyOpenAction(bookmarkLink, in: bookmarksTabID, event: XPCNotificationBridge.openBookmark,
                                   idKey: "bookmarkId", itemID: bookmark.id)

        let remove = try #require(trees[historyTabID]?.nodes.first { $0.label == "Remove \(visit.title)" })
        click(remove, in: historyTabID)
        try await waitForUpdate(historyTabID) { removedVisitID == visit.id.uuidString }

        try await render(.history, tabID: historyTabID)
        try await waitForUpdate(historyTabID) { hasLabel("No history yet", in: historyTabID) }
        #expect(!hasLabel(visit.title, in: historyTabID))
        #expect(hasLabel(bookmark.title, in: bookmarksTabID))
    }

    private func verifyOpenAction(
        _ link: Actrium_Ipc_AccessibilityNode,
        in tabID: UUID,
        event: Notification.Name,
        idKey: String,
        itemID: UUID
    ) async throws {
        var received = false
        let events = NotificationCenter.default.publisher(for: event).sink { notification in
            guard notification.userInfo?["tabId"] as? UUID == tabID,
                  notification.userInfo?[idKey] as? String == itemID.uuidString else { return }
            received = true
        }
        defer { events.cancel() }
        click(link, in: tabID)
        try await waitForUpdate(tabID) { received }
    }

    private func render(
        _ type: SystemTabType,
        tabID: UUID,
        history: [HistoryItem] = [],
        bookmarks: [Bookmark] = []
    ) async throws {
        var config: [String: Any] = [
            "tabId": tabID.uuidString,
            "title": type.title,
            "systemAppType": type.rawValue,
            "containerWidth": 800,
            "containerHeight": 500,
        ]
        for (key, value) in LibraryTabData.configuration(for: type, history: history, bookmarks: bookmarks) {
            config[key] = value
        }
        try await RendererProcessPool.shared.prepareTab(tabId: tabID, config: config)
    }

    private func click(_ node: Actrium_Ipc_AccessibilityNode, in tabID: UUID) {
        var input = Actrium_Ipc_InputEvent()
        input.x = node.x + node.width / 2
        input.y = node.y + node.height / 2
        input.type = .mouseDown
        RendererProcessPool.shared.sendInput(input, to: tabID)
        input.type = .mouseUp
        RendererProcessPool.shared.sendInput(input, to: tabID)
    }

    private func waitForUpdate(_ tabID: UUID, until predicate: () -> Bool) async throws {
        let deadline = ContinuousClock.now + .seconds(5)
        while !predicate(), ContinuousClock.now < deadline {
            RendererProcessPool.shared.requestAccessibilityTree(for: tabID)
            try await Task.sleep(for: .milliseconds(100))
        }
        try #require(predicate())
    }
}
