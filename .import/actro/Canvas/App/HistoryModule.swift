import Foundation
import React

/// Native module for history actions from React Native
@objc(HistoryModule)
class HistoryModule: NSObject {

    @objc
    func openHistoryItem(_ itemId: String, _ appURL: String?) {
        guard let urlString = appURL else {
            print("⚠️ [HistoryModule] URL is nil")
            return
        }

        // Try parsing as URL first, then as file path
        let url: URL
        if let parsed = URL(string: urlString) {
            url = parsed
        } else {
            // Assume it's a file path without file:// prefix
            url = URL(fileURLWithPath: urlString)
        }

        print("📜 [HistoryModule] Opening history item: \(itemId), url: \(url.absoluteString)")

        DispatchQueue.main.async {
            NotificationCenter.default.post(
                name: .openHistoryItemFromRN,
                object: nil,
                userInfo: ["itemId": itemId, "url": url]
            )
        }
    }

    @objc
    func deleteHistoryItem(_ itemId: String) {
        print("🗑️ [HistoryModule] Delete history item: \(itemId)")

        DispatchQueue.main.async {
            NotificationCenter.default.post(
                name: .deleteHistoryItemFromRN,
                object: nil,
                userInfo: ["itemId": itemId]
            )
        }
    }

    @objc
    func restoreGroup(_ groupId: String) {
        print("♻️ [HistoryModule] Restore group: \(groupId)")

        DispatchQueue.main.async {
            NotificationCenter.default.post(
                name: .restoreHistoryGroupFromRN,
                object: nil,
                userInfo: ["groupId": groupId]
            )
        }
    }

    @objc
    func clearHistory() {
        print("🧹 [HistoryModule] Clear all history")

        DispatchQueue.main.async {
            NotificationCenter.default.post(
                name: .clearHistoryFromRN,
                object: nil,
                userInfo: nil
            )
        }
    }

    @objc
    static func requiresMainQueueSetup() -> Bool {
        return false
    }

    @objc
    static func moduleName() -> String! {
        return "HistoryModule"
    }
}

// MARK: - Notification Names

extension Notification.Name {
    static let openHistoryItemFromRN = Notification.Name("openHistoryItemFromRN")
    static let deleteHistoryItemFromRN = Notification.Name("deleteHistoryItemFromRN")
    static let restoreHistoryGroupFromRN = Notification.Name("restoreHistoryGroupFromRN")
    static let clearHistoryFromRN = Notification.Name("clearHistoryFromRN")
}
