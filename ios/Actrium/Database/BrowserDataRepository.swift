import Foundation
import ActriumCore

private let browserDataLogger = AppLogger.database
private let iOSHistoryBackfillKey = "apple_ios_local_history_backfilled_v1"

final class BrowserDataRepository {
    static let shared = BrowserDataRepository()

    private let database: AppleBrowserStorageDatabase?
    private let syncScheduler: AppleBrowserStorageSyncScheduler?
    private let historyReadLimit = 500
    private let bookmarkReadLimit = 500

    private init() {
        let path = Self.databasePath()
        do {
            let openedDatabase = try AppleBrowserStorageDatabase(path: path) { message in
                browserDataLogger.error("\(message)")
            }
            database = openedDatabase
            let syncCoordinator = AppleBrowserStorageSyncCoordinator(
                client: AppleBrowserStorageSyncClient(database: openedDatabase),
                credentialsProvider: BrowserSyncCredentialsProvider.shared
            )
            syncScheduler = AppleBrowserStorageSyncScheduler(coordinator: syncCoordinator)
            browserDataLogger.notice("Initialized browser storage at: \(path)")
        } catch {
            database = nil
            syncScheduler = nil
            browserDataLogger.error("Failed to open browser storage at: \(path), error: \(String(describing: error))")
        }
    }

    func syncNow() async -> String? {
        await syncScheduler?.runNow()
    }

    func syncStatusJson() async -> String? {
        await syncScheduler?.statusJson()
    }

    func scheduleSyncAfterCredentialChange() async {
        await syncScheduler?.scheduleAfterLocalChange(delaySeconds: 0)
    }

    func cancelPendingSync() async {
        await syncScheduler?.cancelPendingSync()
    }

    func recordVisit(tab: Tab) {
        guard let url = tab.url else { return }
        recordVisit(title: tab.title, url: url, visitedAt: Date())
    }

    func recordVisit(item: HistoryItem) {
        guard let url = item.appURL else { return }
        recordVisit(title: item.title, url: url, visitedAt: item.visitTime)
    }

    func loadRecentHistory(limit: Int = 500) -> [HistoryItem] {
        database?.loadRecentHistory(limit: min(max(limit, 0), historyReadLimit)).compactMap(historyItem(from:)) ?? []
    }

    func backfillHistoryIfNeeded(_ items: [HistoryItem]) {
        guard let database, !database.hasMetaFlag(iOSHistoryBackfillKey) else { return }
        let browserItems = items.compactMap(browserHistoryItem(from:))
        guard !browserItems.isEmpty else {
            _ = database.setMetaFlag(iOSHistoryBackfillKey)
            return
        }

        let succeeded = browserItems.allSatisfy { database.upsertHistoryItem($0) }
        if succeeded {
            _ = database.setMetaFlag(iOSHistoryBackfillKey)
            scheduleSyncAfterLocalChange()
            browserDataLogger.notice("Backfilled \(browserItems.count) local history items into browser storage")
        }
    }

    func removeHistory(item: HistoryItem) {
        guard let url = item.appURL, Self.isBrowserPageURL(url) else { return }
        _ = deleteHistory(address: url.absoluteString)
    }

    @discardableResult
    func deleteHistory(address: String) -> Bool {
        guard let url = URL(string: address), Self.isBrowserPageURL(url) else { return false }
        let succeeded = database?.deleteHistory(address: url.absoluteString) == true
        if succeeded {
            scheduleSyncAfterLocalChange()
        }
        return succeeded
    }

    @discardableResult
    func clearHistory() -> Bool {
        let succeeded = database?.clearHistory() == true
        if succeeded {
            scheduleSyncAfterLocalChange()
        }
        return succeeded
    }

    func loadBookmarks(limit: Int = 500) -> [AppleBrowserBookmarkItem] {
        database?.loadRecentBookmarks(limit: min(max(limit, 0), bookmarkReadLimit)) ?? []
    }

    func loadBookmarkPayloads(limit: Int = 500) -> [[String: Any]] {
        loadBookmarks(limit: limit).map { item in
            [
                "id": item.id,
                "title": item.title,
                "appURL": item.address,
                "createdAt": Double(item.createdAtUnixSeconds) * 1000,
                "lastUsed": Double(item.updatedAtUnixSeconds) * 1000,
            ]
        }
    }

    @discardableResult
    func deleteBookmark(address: String) -> Bool {
        guard let url = URL(string: address), Self.isBrowserPageURL(url) else { return false }
        let succeeded = database?.deleteBookmark(address: url.absoluteString) == true
        if succeeded {
            scheduleSyncAfterLocalChange()
        }
        return succeeded
    }

    @discardableResult
    func clearBookmarks() -> Bool {
        let succeeded = database?.clearBookmarks() == true
        if succeeded {
            scheduleSyncAfterLocalChange()
        }
        return succeeded
    }

    func saveBookmarkedTabs(_ tabs: [Tab]) {
        guard let database else { return }
        let now = Int64(Date().timeIntervalSince1970)
        let items = tabs.enumerated().compactMap { index, tab -> AppleBrowserBookmarkItem? in
            guard tab.isBookmarked,
                  let url = tab.url,
                  Self.isBrowserPageURL(url) else { return nil }
            return AppleBrowserBookmarkItem(
                id: tab.id.uuidString,
                title: normalizedTitle(tab.title, url: url),
                address: url.absoluteString,
                createdAtUnixSeconds: now,
                updatedAtUnixSeconds: now,
                sortOrder: Int32(index),
                favicon: tab.icon,
                ownerKind: "tab",
                ownerId: tab.id.uuidString
            )
        }
        let bookmarkedIds = Set(items.map(\.id))
        let browserPageTabIds = Set(tabs.compactMap { tab -> String? in
            guard let url = tab.url, Self.isBrowserPageURL(url) else { return nil }
            return tab.id.uuidString
        })
        let staleTabBookmarkIds = browserPageTabIds.subtracting(bookmarkedIds)

        var changed = false
        for item in items {
            changed = database.upsertBookmarkItem(item) || changed
        }
        for bookmarkId in staleTabBookmarkIds {
            changed = database.deleteBookmark(id: bookmarkId) || changed
        }
        if changed {
            scheduleSyncAfterLocalChange()
        }
    }

    func saveSession(tabs: [Tab], currentTabId: UUID?) {
        let now = Int64(Date().timeIntervalSince1970)
        let browserTabs = tabs.compactMap { tab -> (id: UUID, item: AppleBrowserSessionTab)? in
            guard let url = tab.url, Self.isBrowserPageURL(url) else { return nil }
            let item = AppleBrowserSessionTab(
                id: tab.id.uuidString,
                title: normalizedTitle(tab.title, url: url),
                address: url.absoluteString,
                createdAtUnixSeconds: now,
                updatedAtUnixSeconds: now
            )
            return (tab.id, item)
        }

        let activeTabIndex: Int32
        if let currentTabId,
           let index = browserTabs.firstIndex(where: { $0.id == currentTabId }) {
            activeTabIndex = Int32(index)
        } else {
            activeTabIndex = 0
        }

        let snapshot = AppleBrowserSessionSnapshot(
            activeTabIndex: activeTabIndex,
            tabs: browserTabs.map { $0.item },
            surfaceKind: "ios"
        )
        if database?.saveSession(snapshot: snapshot) == true {
            scheduleSyncAfterLocalChange()
        }
    }

    func loadSessionTabs() -> (tabs: [Tab], currentTabId: UUID?)? {
        guard let snapshot = database?.loadSession() else { return nil }
        let tabs = snapshot.tabs.compactMap { item -> Tab? in
            guard let url = URL(string: item.address), Self.isBrowserPageURL(url) else { return nil }
            return Tab(
                id: UUID(uuidString: item.id) ?? stableUUID(for: "session:\(item.id):\(item.address)"),
                title: normalizedTitle(item.title, url: url),
                url: url
            )
        }
        guard !tabs.isEmpty else { return nil }

        let activeIndex = Int(snapshot.activeTabIndex)
        let currentTabId = tabs.indices.contains(activeIndex) ? tabs[activeIndex].id : tabs.first?.id
        return (tabs, currentTabId)
    }

    private func recordVisit(title: String, url: URL, visitedAt: Date) {
        guard Self.isBrowserPageURL(url) else { return }
        let item = AppleBrowserHistoryItem(
            title: normalizedTitle(title, url: url),
            address: url.absoluteString,
            visitedAtUnixSeconds: Int64(visitedAt.timeIntervalSince1970),
            visitCount: 1
        )
        if database?.recordHistoryVisit(item) == true {
            scheduleSyncAfterLocalChange()
        }
    }

    private func scheduleSyncAfterLocalChange() {
        guard let syncScheduler else { return }
        Task {
            await syncScheduler.scheduleAfterLocalChange()
        }
    }

    private static func databasePath() -> String {
        let fileManager = FileManager.default
        let appSupport = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let appFolder = appSupport.appendingPathComponent("Actrium", isDirectory: true)
        if !fileManager.fileExists(atPath: appFolder.path) {
            try? fileManager.createDirectory(at: appFolder, withIntermediateDirectories: true)
        }
        return AppleBrowserStorageDatabase.databasePath(in: appFolder)
    }

    static func isBrowserPageURL(_ url: URL) -> Bool {
        let scheme = url.scheme?.lowercased()
        return scheme == "http" || scheme == "https"
    }

    private func historyItem(from item: AppleBrowserHistoryItem) -> HistoryItem? {
        guard let url = URL(string: item.address), Self.isBrowserPageURL(url) else { return nil }
        return HistoryItem(
            id: stableUUID(for: "history:\(item.address)"),
            title: normalizedTitle(item.title, url: url),
            appURL: url,
            visitTime: Date(timeIntervalSince1970: TimeInterval(item.visitedAtUnixSeconds))
        )
    }

    private func browserHistoryItem(from item: HistoryItem) -> AppleBrowserHistoryItem? {
        guard let url = item.appURL, Self.isBrowserPageURL(url) else { return nil }
        return AppleBrowserHistoryItem(
            title: normalizedTitle(item.title, url: url),
            address: url.absoluteString,
            visitedAtUnixSeconds: Int64(item.visitTime.timeIntervalSince1970),
            visitCount: 1
        )
    }

    private func normalizedTitle(_ title: String, url: URL) -> String {
        let trimmed = title.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmed.isEmpty {
            return trimmed
        }
        if let host = url.host, !host.isEmpty {
            return host
        }
        return url.absoluteString
    }

    private func stableUUID(for value: String) -> UUID {
        let bytes = stableIdentifierBytes(for: value)
        return UUID(uuid: (
            bytes[0], bytes[1], bytes[2], bytes[3],
            bytes[4], bytes[5], bytes[6], bytes[7],
            bytes[8], bytes[9], bytes[10], bytes[11],
            bytes[12], bytes[13], bytes[14], bytes[15]
        ))
    }

    private func stableIdentifierBytes(for value: String) -> [UInt8] {
        let first = fnv1a64(value.utf8)
        let second = fnv1a64(("actrium:" + value).utf8)
        var bytes = [UInt8](repeating: 0, count: 16)
        for index in 0..<8 {
            bytes[index] = UInt8((first >> UInt64((7 - index) * 8)) & 0xff)
            bytes[index + 8] = UInt8((second >> UInt64((7 - index) * 8)) & 0xff)
        }
        bytes[6] = (bytes[6] & 0x0f) | 0x50
        bytes[8] = (bytes[8] & 0x3f) | 0x80
        return bytes
    }

    private func fnv1a64<T: Sequence>(_ bytes: T) -> UInt64 where T.Element == UInt8 {
        var hash: UInt64 = 0xcbf29ce484222325
        for byte in bytes {
            hash ^= UInt64(byte)
            hash = hash &* 0x100000001b3
        }
        return hash
    }
}
