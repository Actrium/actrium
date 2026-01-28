import Foundation
import React

private let logger = AppLogger.reactNative

@objc(TabModule)
class TabModule: NSObject {

    @objc
    func openTab(_ options: NSDictionary, resolve: @escaping RCTPromiseResolveBlock, reject: @escaping RCTPromiseRejectBlock) {
        logger.notice("[TabModule] openTab called with options: \(options)")
        
        guard let systemAppType = options["systemAppType"] as? String else {
            reject("invalid_options", "systemAppType is required", nil)
            return
        }
        
        // Generate a new tab ID to return to JS
        let newTabId = UUID()
        
        DispatchQueue.main.async {
            NotificationCenter.default.post(
                name: .openTabFromRN,
                object: nil,
                userInfo: [
                    "tabId": newTabId,
                    "systemAppType": systemAppType
                ]
            )
            // Resolve with the new tab ID so JS can register tasks
            resolve(newTabId.uuidString)
        }
    }

    @objc
    func selectTab(_ tabId: String, resolve: @escaping RCTPromiseResolveBlock, reject: @escaping RCTPromiseRejectBlock) {
        logger.notice("[TabModule] selectTab called with tabId: \(tabId)")
        
        DispatchQueue.main.async {
            NotificationCenter.default.post(
                name: .selectTabFromRN,
                object: nil,
                userInfo: [
                    "tabId": tabId
                ]
            )
            resolve(true)
        }
    }

    @objc
    func closeTab(_ tabId: String, resolve: @escaping RCTPromiseResolveBlock, reject: @escaping RCTPromiseRejectBlock) {
        logger.notice("[TabModule] closeTab called with tabId: \(tabId)")
        
        DispatchQueue.main.async {
            NotificationCenter.default.post(
                name: .closeTabFromRN,
                object: nil,
                userInfo: [
                    "tabId": tabId
                ]
            )
            resolve(true)
        }
    }

    @objc
    static func requiresMainQueueSetup() -> Bool {
        return false
    }
}

extension Notification.Name {
    static let openTabFromRN = Notification.Name("openTabFromRN")
    static let selectTabFromRN = Notification.Name("selectTabFromRN")
    static let closeTabFromRN = Notification.Name("closeTabFromRN")
}
