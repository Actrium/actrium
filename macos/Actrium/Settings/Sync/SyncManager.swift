import Foundation
import AppKit
import Combine

private let logger = AppLogger.database

// Drives the Settings > Sync "Local Folder" surface. It owns folder
// selection (NSOpenPanel + security-scoped bookmark) and observable status,
// and delegates the actual sync to BrowserDataRepository, which runs the Rust
// local-folder cloud-provider rounds against the picked folder. This replaces
// the former SyncBundle exporter, which hand-merged bookmarks/tags/vault/app
// settings into an actrium-sync.json file; the browser-storage sync engine
// syncs browser history, browser bookmarks, and session tabs instead.
final class SyncManager: ObservableObject {
    static let shared = SyncManager()

    // MARK: - Published State

    @Published private(set) var syncFolderPath: String?
    @Published private(set) var lastSyncDate: Date?
    @Published private(set) var isSyncing: Bool = false
    @Published private(set) var syncError: String?
    @Published private(set) var statusText: String = "Not configured"

    private let lastSyncDateKey = "browserSyncProviderLastSyncDate"

    // MARK: - Init

    private init() {
        if let interval = UserDefaults.standard.object(forKey: lastSyncDateKey) as? Double {
            lastSyncDate = Date(timeIntervalSince1970: interval)
        }
    }

    var isConfigured: Bool {
        syncFolderPath != nil
    }

    // MARK: - Lifecycle

    func restoreOnLaunch() {
        let snapshot = BrowserDataRepository.shared.localFolderProviderSnapshot()
        if snapshot.isConfigured, let path = snapshot.localFolderRootPath {
            syncFolderPath = path
            statusText = lastSyncDate == nil ? "Ready" : "Up to date"
        } else {
            syncFolderPath = nil
            statusText = "Not configured"
        }
    }

    // MARK: - Folder selection

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
            _ = BrowserDataRepository.shared.configureLocalFolderProvider(
                rootURL: url,
                bookmarkData: bookmarkData
            )
            syncFolderPath = url.path
            syncError = nil
            statusText = "Ready"
            logger.notice("Local-folder browser sync folder set to: \(url.path)")

            // A freshly picked folder runs the membership join and first
            // cycle immediately so the user sees a real outcome instead of
            // waiting for the next trigger.
            Task { await self.syncNow() }
        } catch {
            syncError = "Failed to save folder access: \(error.localizedDescription)"
            logger.error("Failed to create security-scoped bookmark: \(error)")
        }
    }

    func removeSyncFolder() {
        BrowserDataRepository.shared.clearLocalFolderProvider()
        syncFolderPath = nil
        syncError = nil
        statusText = "Not configured"
        logger.notice("Local-folder browser sync folder removed")
    }

    // MARK: - Sync

    @MainActor
    func syncNow() async {
        guard isConfigured, !isSyncing else { return }
        isSyncing = true
        syncError = nil

        let resultJson = await BrowserDataRepository.shared.runLocalFolderSyncCycleNow()
        apply(resultJson: resultJson)
        isSyncing = false
    }

    private func apply(resultJson: String?) {
        guard let resultJson,
              let data = resultJson.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data),
              let payload = object as? [String: Any]
        else {
            syncError = "Sync did not run"
            statusText = "Unavailable"
            return
        }

        let classification = payload["classification"] as? String ?? ""
        let applied = (payload["applied"] as? NSNumber)?.boolValue ?? false
        let clientAction = payload["clientAction"] as? String ?? ""

        if Self.isSuccessClassification(classification) {
            let now = Date()
            lastSyncDate = now
            UserDefaults.standard.set(now.timeIntervalSince1970, forKey: lastSyncDateKey)
            syncError = nil
            statusText = applied ? "Synced" : "Up to date"
        } else {
            let display = Self.displayStatus(classification, clientAction: clientAction)
            statusText = display
            syncError = Self.isBlockedClassification(classification) ? display : nil
        }
    }

    private static func isSuccessClassification(_ classification: String) -> Bool {
        switch classification {
        case "ok", "success", "synced", "idle", "upToDate":
            return true
        default:
            return false
        }
    }

    private static func isBlockedClassification(_ classification: String) -> Bool {
        switch classification {
        case "clientUpgradeRequired",
             "adapterUnsupported",
             "storageSetupRequired",
             "recoveryRequired",
             "providerAuthRequired",
             "providerQuotaExceeded":
            return true
        default:
            return false
        }
    }

    private static func displayStatus(_ classification: String, clientAction: String) -> String {
        switch classification {
        case "clientUpgradeRequired":
            return "App update required"
        case "adapterUnsupported":
            return "Folder not supported"
        case "storageSetupRequired", "recoveryRequired":
            return "Folder needs setup"
        case "providerAuthRequired":
            return "Folder access required"
        case "providerQuotaExceeded":
            return "Folder is full"
        case "":
            return clientAction.isEmpty ? "Unavailable" : clientAction
        default:
            return classification
        }
    }
}
