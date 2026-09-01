import Foundation
import React

private let browserDataModuleLogger = AppLogger.reactNative
private let browserDataModuleQueue = DispatchQueue(
    label: "io.actrium.actro.browser-data-module",
    qos: .userInitiated
)

@objc(BrowserDataModule)
final class BrowserDataModule: NSObject {
    @objc
    func getRecentHistory(
        _ limit: NSNumber,
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        browserDataModuleQueue.async {
            let items = BrowserDataRepository.shared.loadRecentHistory(limit: limit.intValue)
            resolve(items.map(Self.historyPayload))
        }
    }

    @objc
    func deleteHistoryItem(
        _ address: String,
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        browserDataModuleQueue.async {
            resolve(BrowserDataRepository.shared.deleteHistory(address: address))
        }
    }

    @objc
    func clearHistory(
        _ resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        browserDataModuleQueue.async {
            resolve(BrowserDataRepository.shared.clearHistory())
        }
    }

    @objc
    func getBookmarks(
        _ limit: NSNumber,
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        browserDataModuleQueue.async {
            resolve(BrowserDataRepository.shared.loadBookmarkPayloads(limit: limit.intValue))
        }
    }

    @objc
    func deleteBookmark(
        _ address: String,
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        browserDataModuleQueue.async {
            resolve(BrowserDataRepository.shared.deleteBookmark(address: address))
        }
    }

    @objc
    func clearBookmarks(
        _ resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        browserDataModuleQueue.async {
            resolve(BrowserDataRepository.shared.clearBookmarks())
        }
    }

    @objc
    func saveSyncSession(
        _ environment: String,
        token: String,
        baseUrl: String,
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        BrowserSyncCredentialsProvider.shared.save(
            environment: environment,
            accessToken: token,
            baseUrl: baseUrl
        )
        Task {
            await BrowserDataRepository.shared.scheduleSyncAfterCredentialChange()
            resolve(true)
        }
    }

    @objc
    func clearSyncSession(
        _ environment: String,
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        BrowserSyncCredentialsProvider.shared.clear(environment: environment)
        Task {
            await BrowserDataRepository.shared.cancelPendingSync()
            resolve(true)
        }
    }

    @objc
    func syncNow(
        _ resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        Task {
            browserDataModuleLogger.notice("[BrowserDataModule] Running browser data sync")
            let resultJson = await BrowserDataRepository.shared.syncNow()
            resolve(resultJson ?? NSNull())
        }
    }

    @objc
    func getSyncStatus(
        _ resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        Task {
            let statusJson = await BrowserDataRepository.shared.syncStatusJson()
            resolve(statusJson ?? NSNull())
        }
    }

    @objc
    static func requiresMainQueueSetup() -> Bool {
        false
    }

    @objc
    static func moduleName() -> String! {
        "BrowserDataModule"
    }

    private static func historyPayload(_ item: HistoryItem) -> [String: Any] {
        var payload: [String: Any] = [
            "id": item.id.uuidString,
            "title": item.title,
            "visitTime": item.visitTime.timeIntervalSince1970 * 1000,
        ]
        if let url = item.appURL {
            payload["appURL"] = url.absoluteString
        }
        return payload
    }

}
