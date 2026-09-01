import Foundation
import React

private let logger = AppLogger.reactNative

/// Native module for bookmark actions from React Native
@objc(BookmarksModule)
class BookmarksModule: NSObject {

    @objc
    func openBookmark(_ bookmarkId: String, _ appURL: String?) {
        guard let urlString = appURL else {
            logger.warning("[BookmarksModule] URL is nil")
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

        logger.info("[BookmarksModule] Opening bookmark: \(bookmarkId), url: \(url.absoluteString)")

        DispatchQueue.main.async {
            NotificationCenter.default.post(
                name: .openBookmarkFromRN,
                object: nil,
                userInfo: ["bookmarkId": bookmarkId, "url": url]
            )
        }
    }

    @objc
    func deleteBookmark(_ bookmarkId: String) {
        logger.notice("[BookmarksModule] Delete bookmark: \(bookmarkId)")

        DispatchQueue.main.async {
            NotificationCenter.default.post(
                name: .deleteBookmarkFromRN,
                object: nil,
                userInfo: ["bookmarkId": bookmarkId]
            )
        }
    }

    @objc
    static func requiresMainQueueSetup() -> Bool {
        return false
    }

    @objc
    static func moduleName() -> String! {
        return "BookmarksModule"
    }
}

// MARK: - Notification Names

extension Notification.Name {
    static let openBookmarkFromRN = Notification.Name("openBookmarkFromRN")
    static let deleteBookmarkFromRN = Notification.Name("deleteBookmarkFromRN")
}
