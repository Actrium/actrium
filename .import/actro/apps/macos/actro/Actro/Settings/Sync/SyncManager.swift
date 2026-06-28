import Foundation
import AppKit
import Combine

private let logger = AppLogger.database

final class SyncManager: ObservableObject {
    static let shared = SyncManager()

    // MARK: - Published State

    @Published var syncFolderURL: URL?
    @Published var lastSyncDate: Date?
    @Published var isSyncing: Bool = false
    @Published var syncError: String?

    @Published var autoSyncEnabled: Bool {
        didSet { UserDefaults.standard.set(autoSyncEnabled, forKey: "syncAutoSyncEnabled") }
    }
    @Published var syncBookmarks: Bool {
        didSet { UserDefaults.standard.set(syncBookmarks, forKey: "syncBookmarksEnabled") }
    }
    @Published var syncTags: Bool {
        didSet { UserDefaults.standard.set(syncTags, forKey: "syncTagsEnabled") }
    }
    @Published var syncSettings: Bool {
        didSet { UserDefaults.standard.set(syncSettings, forKey: "syncSettingsEnabled") }
    }
    @Published var syncVault: Bool {
        didSet { UserDefaults.standard.set(syncVault, forKey: "syncVaultEnabled") }
    }

    // MARK: - Private

    private var fileMonitorSource: DispatchSourceFileSystemObject?
    private let syncFileName = "actro-sync.json"

    private var deviceId: String {
        if let existing = UserDefaults.standard.string(forKey: "syncDeviceId") {
            return existing
        }
        let newId = UUID().uuidString
        UserDefaults.standard.set(newId, forKey: "syncDeviceId")
        return newId
    }

    // MARK: - Init

    private init() {
        self.autoSyncEnabled = UserDefaults.standard.bool(forKey: "syncAutoSyncEnabled")
        self.syncBookmarks = UserDefaults.standard.object(forKey: "syncBookmarksEnabled") as? Bool ?? true
        self.syncTags = UserDefaults.standard.object(forKey: "syncTagsEnabled") as? Bool ?? true
        self.syncSettings = UserDefaults.standard.object(forKey: "syncSettingsEnabled") as? Bool ?? true
        self.syncVault = UserDefaults.standard.object(forKey: "syncVaultEnabled") as? Bool ?? true

        if let dateInterval = UserDefaults.standard.object(forKey: "syncLastSyncDate") as? Double {
            self.lastSyncDate = Date(timeIntervalSince1970: dateInterval)
        }
    }

    // MARK: - Public Methods

    func restoreOnLaunch() {
        restoreSyncFolder()
        if autoSyncEnabled && syncFolderURL != nil {
            startMonitoring()
        }
    }

    func selectSyncFolder() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false
        panel.prompt = "Choose Sync Folder"

        guard panel.runModal() == .OK, let url = panel.url else { return }

        do {
            let bookmarkData = try url.bookmarkData(
                options: .withSecurityScope,
                includingResourceValuesForKeys: nil,
                relativeTo: nil
            )
            UserDefaults.standard.set(bookmarkData, forKey: "syncFolderBookmark")
            syncFolderURL = url
            syncError = nil
            logger.notice("Sync folder set to: \(url.path)")

            if autoSyncEnabled {
                startMonitoring()
            }
        } catch {
            syncError = "Failed to save folder access: \(error.localizedDescription)"
            logger.error("Failed to create security-scoped bookmark: \(error)")
        }
    }

    func removeSyncFolder() {
        stopMonitoring()
        if let url = syncFolderURL {
            url.stopAccessingSecurityScopedResource()
        }
        syncFolderURL = nil
        UserDefaults.standard.removeObject(forKey: "syncFolderBookmark")
        syncError = nil
        logger.notice("Sync folder removed")
    }

    func exportNow() {
        guard let folderURL = syncFolderURL else {
            syncError = "No sync folder selected"
            return
        }

        isSyncing = true
        syncError = nil

        let bookmarks = syncBookmarks ? gatherBookmarks() : []
        let tags = syncTags ? gatherTags() : []
        let settings = syncSettings ? gatherSettings() : SyncSettings()
        let vaultMeta = syncVault ? gatherVaultMeta() : nil
        let vaultItems = syncVault ? gatherVaultItems() : nil

        let bundle = SyncBundle(
            version: 1,
            deviceId: deviceId,
            exportedAt: Date(),
            bookmarks: bookmarks,
            tags: tags,
            settings: settings,
            vaultMeta: vaultMeta,
            vaultItems: vaultItems
        )

        do {
            let encoder = JSONEncoder()
            encoder.dateEncodingStrategy = .iso8601
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
            let data = try encoder.encode(bundle)

            let fileURL = folderURL.appendingPathComponent(syncFileName)
            try data.write(to: fileURL, options: .atomic)

            lastSyncDate = Date()
            UserDefaults.standard.set(lastSyncDate!.timeIntervalSince1970, forKey: "syncLastSyncDate")
            logger.notice("Export completed: \(bookmarks.count) bookmarks, \(tags.count) tags")
        } catch {
            syncError = "Export failed: \(error.localizedDescription)"
            logger.error("Export failed: \(error)")
        }

        isSyncing = false
    }

    func importNow() {
        guard let folderURL = syncFolderURL else {
            syncError = "No sync folder selected"
            return
        }

        let fileURL = folderURL.appendingPathComponent(syncFileName)
        guard FileManager.default.fileExists(atPath: fileURL.path) else {
            syncError = "No sync file found"
            return
        }

        isSyncing = true
        syncError = nil

        do {
            let data = try Data(contentsOf: fileURL)
            let decoder = JSONDecoder()
            decoder.dateDecodingStrategy = .iso8601
            let bundle = try decoder.decode(SyncBundle.self, from: data)

            mergeImportedData(bundle)

            lastSyncDate = Date()
            UserDefaults.standard.set(lastSyncDate!.timeIntervalSince1970, forKey: "syncLastSyncDate")
            logger.notice("Import completed from device: \(bundle.deviceId)")
        } catch {
            syncError = "Import failed: \(error.localizedDescription)"
            logger.error("Import failed: \(error)")
        }

        isSyncing = false
    }

    func startMonitoring() {
        guard let folderURL = syncFolderURL else { return }
        stopMonitoring()

        let fileURL = folderURL.appendingPathComponent(syncFileName)
        let fd = open(fileURL.path, O_EVTONLY)
        guard fd >= 0 else {
            // File may not exist yet; monitor the directory instead
            let dirFd = open(folderURL.path, O_EVTONLY)
            guard dirFd >= 0 else { return }

            let source = DispatchSource.makeFileSystemObjectSource(
                fileDescriptor: dirFd,
                eventMask: .write,
                queue: .main
            )
            source.setEventHandler { [weak self] in
                guard let self else { return }
                Task { @MainActor in
                    if FileManager.default.fileExists(atPath: fileURL.path) {
                        self.importNow()
                    }
                }
            }
            source.setCancelHandler { close(dirFd) }
            source.resume()
            fileMonitorSource = source
            return
        }

        let source = DispatchSource.makeFileSystemObjectSource(
            fileDescriptor: fd,
            eventMask: .write,
            queue: .main
        )
        source.setEventHandler { [weak self] in
            guard let self else { return }
            Task { @MainActor in
                self.importNow()
            }
        }
        source.setCancelHandler { close(fd) }
        source.resume()
        fileMonitorSource = source
        logger.notice("File monitoring started")
    }

    func stopMonitoring() {
        fileMonitorSource?.cancel()
        fileMonitorSource = nil
    }

    // MARK: - Private: Restore

    private func restoreSyncFolder() {
        guard let data = UserDefaults.standard.data(forKey: "syncFolderBookmark") else { return }

        do {
            var isStale = false
            let url = try URL(
                resolvingBookmarkData: data,
                options: .withSecurityScope,
                relativeTo: nil,
                bookmarkDataIsStale: &isStale
            )
            _ = url.startAccessingSecurityScopedResource()

            if isStale {
                // Re-save the bookmark
                let newData = try url.bookmarkData(
                    options: .withSecurityScope,
                    includingResourceValuesForKeys: nil,
                    relativeTo: nil
                )
                UserDefaults.standard.set(newData, forKey: "syncFolderBookmark")
            }

            syncFolderURL = url
            logger.notice("Restored sync folder: \(url.path)")
        } catch {
            logger.error("Failed to restore sync folder bookmark: \(error)")
            syncError = "Could not restore sync folder access"
        }
    }

    // MARK: - Private: Gather Data

    private func gatherBookmarks() -> [SyncBookmark] {
        let repo = BookmarkRepository()
        let tagRepo = TagRepository()
        let bookmarks = repo.getAll()

        return bookmarks.map { bookmark in
            let tagIds = tagRepo.getTagsForBookmark(bookmarkId: bookmark.id).map { $0.id.uuidString }
            return SyncBookmark(
                id: bookmark.id.uuidString,
                title: bookmark.title,
                appURL: bookmark.appURL?.absoluteString,
                appId: bookmark.appId,
                createdAt: bookmark.createdAt,
                lastUsedAt: bookmark.lastUsedAt,
                manufacturer: bookmark.manufacturer,
                notes: bookmark.notes,
                favicon: bookmark.favicon,
                tagIds: tagIds
            )
        }
    }

    private func gatherTags() -> [SyncTag] {
        let repo = TagRepository()
        return repo.getAllTags().map { tag in
            SyncTag(
                id: tag.id.uuidString,
                name: tag.name,
                color: tag.color,
                isSystem: tag.isSystem
            )
        }
    }

    private func gatherSettings() -> SyncSettings {
        SyncSettings(
            autoLockTimeout: UserDefaults.standard.object(forKey: "autoLockTimeout") as? Int,
            customShortcuts: UserDefaults.standard.dictionary(forKey: "customShortcuts") as? [String: String]
        )
    }

    private func gatherVaultMeta() -> [SyncVaultMeta] {
        VaultDatabase.fetchAllVaultMeta().compactMap { row in
            guard let key = row["key"] as? String,
                  let value = row["value"] as? String else { return nil }
            return SyncVaultMeta(key: key, value: value)
        }
    }

    private func gatherVaultItems() -> [SyncVaultItem] {
        VaultDatabase.fetchAllVaultItems().compactMap { row in
            guard let id = row["id"] as? String,
                  let title = row["title"] as? String,
                  let category = row["category"] as? String,
                  let fieldsEnc = row["fields_encrypted"] as? String,
                  let createdAt = row["created_at"] as? Int64,
                  let updatedAt = row["updated_at"] as? Int64 else { return nil }
            return SyncVaultItem(
                id: id,
                title: title,
                category: category,
                fieldsEncrypted: fieldsEnc,
                notes: row["notes"] as? String,
                createdAt: Date(timeIntervalSince1970: TimeInterval(createdAt)),
                updatedAt: Date(timeIntervalSince1970: TimeInterval(updatedAt))
            )
        }
    }

    // MARK: - Private: Merge

    private func mergeImportedData(_ bundle: SyncBundle) {
        let db = DatabaseManager.shared
        let tagRepo = TagRepository()
        let bookmarkRepo = BookmarkRepository()

        // Merge tags first (bookmarks reference them)
        if syncTags {
            let existingTags = tagRepo.getAllTags()
            let existingTagIds = Set(existingTags.map { $0.id.uuidString })

            for syncTag in bundle.tags {
                if existingTagIds.contains(syncTag.id) {
                    // Update existing tag (skip system tags)
                    if !syncTag.isSystem {
                        db.execute(
                            "UPDATE tags SET name = ?, color = ? WHERE id = ? AND is_system = 0",
                            params: [syncTag.name, syncTag.color, syncTag.id]
                        )
                    }
                } else {
                    // Insert new tag
                    db.execute(
                        "INSERT OR IGNORE INTO tags (id, name, color, is_system) VALUES (?, ?, ?, ?)",
                        params: [syncTag.id, syncTag.name, syncTag.color, syncTag.isSystem ? 1 : 0]
                    )
                }
            }
        }

        // Merge bookmarks
        if syncBookmarks {
            let existingBookmarks = bookmarkRepo.getAll()
            let existingById = Dictionary(uniqueKeysWithValues: existingBookmarks.map { ($0.id.uuidString, $0) })

            for syncBookmark in bundle.bookmarks {
                if let existing = existingById[syncBookmark.id] {
                    // Newer wins: compare createdAt (since we don't have an updatedAt field,
                    // use lastUsedAt as a proxy for "most recently touched")
                    let existingTimestamp = existing.lastUsedAt ?? existing.createdAt
                    let incomingTimestamp = syncBookmark.lastUsedAt ?? syncBookmark.createdAt
                    if incomingTimestamp > existingTimestamp {
                        db.execute(
                            """
                            UPDATE bookmarks SET title = ?, app_url = ?, app_id = ?,
                            last_used_at = ?, manufacturer = ?, notes = ?, favicon = ?
                            WHERE id = ?
                            """,
                            params: [
                                syncBookmark.title,
                                syncBookmark.appURL,
                                syncBookmark.appId,
                                syncBookmark.lastUsedAt.map { Int64($0.timeIntervalSince1970) },
                                syncBookmark.manufacturer,
                                syncBookmark.notes,
                                syncBookmark.favicon,
                                syncBookmark.id
                            ]
                        )
                        // Update tag associations
                        let tagUUIDs = syncBookmark.tagIds.compactMap { UUID(uuidString: $0) }
                        if let bookmarkUUID = UUID(uuidString: syncBookmark.id) {
                            tagRepo.setTagsForBookmark(bookmarkId: bookmarkUUID, tagIds: tagUUIDs)
                        }
                    }
                } else {
                    // Insert new bookmark
                    db.execute(
                        """
                        INSERT INTO bookmarks (id, title, app_url, app_id, created_at, last_used_at,
                        manufacturer, notes, favicon) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                        """,
                        params: [
                            syncBookmark.id,
                            syncBookmark.title,
                            syncBookmark.appURL,
                            syncBookmark.appId,
                            Int64(syncBookmark.createdAt.timeIntervalSince1970),
                            syncBookmark.lastUsedAt.map { Int64($0.timeIntervalSince1970) },
                            syncBookmark.manufacturer,
                            syncBookmark.notes,
                            syncBookmark.favicon
                        ]
                    )
                    // Set tag associations
                    let tagUUIDs = syncBookmark.tagIds.compactMap { UUID(uuidString: $0) }
                    if let bookmarkUUID = UUID(uuidString: syncBookmark.id) {
                        tagRepo.setTagsForBookmark(bookmarkId: bookmarkUUID, tagIds: tagUUIDs)
                    }
                }
            }
        }

        // Merge vault data (encrypted as-is, same master password required)
        if syncVault {
            if let metaEntries = bundle.vaultMeta {
                for entry in metaEntries {
                    VaultDatabase.setVaultMeta(key: entry.key, value: entry.value)
                }
            }
            if let items = bundle.vaultItems {
                let existingRows = VaultDatabase.fetchAllVaultItems()
                let existingById = Dictionary(uniqueKeysWithValues: existingRows.compactMap { row -> (String, Int64)? in
                    guard let id = row["id"] as? String, let updatedAt = row["updated_at"] as? Int64 else { return nil }
                    return (id, updatedAt)
                })

                for item in items {
                    let incomingUpdated = Int64(item.updatedAt.timeIntervalSince1970)
                    if let existingUpdated = existingById[item.id] {
                        if incomingUpdated > existingUpdated {
                            VaultDatabase.updateVaultItem(
                                id: item.id,
                                title: item.title,
                                category: item.category,
                                fieldsEncrypted: item.fieldsEncrypted,
                                notes: item.notes,
                                updatedAt: incomingUpdated
                            )
                        }
                    } else {
                        VaultDatabase.insertVaultItem(
                            id: item.id,
                            title: item.title,
                            category: item.category,
                            fieldsEncrypted: item.fieldsEncrypted,
                            notes: item.notes,
                            createdAt: Int64(item.createdAt.timeIntervalSince1970),
                            updatedAt: incomingUpdated
                        )
                    }
                }
            }
        }

        // Merge settings
        if syncSettings {
            if let timeout = bundle.settings.autoLockTimeout {
                UserDefaults.standard.set(timeout, forKey: "autoLockTimeout")
            }
            if let shortcuts = bundle.settings.customShortcuts {
                UserDefaults.standard.set(shortcuts, forKey: "customShortcuts")
            }
        }
    }
}
