import Foundation
import React

private let logger = AppLogger.reactNative

private enum TabEventContract {
    static let tabId = RuntimeBridgeContract.tabId
    static let title = RuntimeBridgeContract.title
    static let icon = RuntimeBridgeContract.icon
    static let appUrl = RuntimeBridgeContract.appUrl
    static let isBookmarked = RuntimeBridgeContract.isBookmarked
    static let enteredFullscreen = RuntimeBridgeContract.tabEnteredFullscreenEvent
    static let exitedFullscreen = RuntimeBridgeContract.tabExitedFullscreenEvent
    static let bookmarkChanged = RuntimeBridgeContract.tabBookmarkChangedEvent
    static let searchInputChanged = RuntimeBridgeContract.searchInputChangedEvent
}

/// Tab Event Emitter - 向 RN 发送 tab 相关事件
@objc(TabEventEmitter)
public class TabEventEmitter: RCTEventEmitter {
    
    static var shared: TabEventEmitter?
    
    override init() {
        super.init()
        TabEventEmitter.shared = self
        setupNotificationObservers()
    }
    
    deinit {
        NotificationCenter.default.removeObserver(self)
    }
    
    // MARK: - RCTEventEmitter Required Methods
    
    @objc public override func supportedEvents() -> [String] {
        return [
            "onTabOpened",
            "onTabClosed",
            "onTabSwitched",
            "onTabUpdated",
            "onTabsReordered",
            TabEventContract.enteredFullscreen,
            TabEventContract.exitedFullscreen,
            TabEventContract.bookmarkChanged,
            TabEventContract.searchInputChanged
        ]
    }
    
    @objc public override static func requiresMainQueueSetup() -> Bool {
        return true
    }
    
    // MARK: - Notification Observers
    
    private func setupNotificationObservers() {
        // Tab opened
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleTabOpened(_:)),
            name: .tabOpenedFromNative,
            object: nil
        )
        
        // Tab closed
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleTabClosed(_:)),
            name: .tabClosedFromNative,
            object: nil
        )
        
        // Tab switched
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleTabSwitched(_:)),
            name: .tabSwitchedFromNative,
            object: nil
        )
        
        // Tab updated
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleTabUpdated(_:)),
            name: .tabUpdatedFromNative,
            object: nil
        )
        
        // Tabs reordered
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleTabsReordered(_:)),
            name: .tabsReorderedFromNative,
            object: nil
        )
        
        // Tab entered fullscreen
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleTabEnteredFullscreen(_:)),
            name: .tabEnteredFullscreenFromNative,
            object: nil
        )
        
        // Tab exited fullscreen
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleTabExitedFullscreen(_:)),
            name: .tabExitedFullscreenFromNative,
            object: nil
        )
        
        // Tab bookmark changed
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleTabBookmarkChanged(_:)),
            name: .tabBookmarkChangedFromNative,
            object: nil
        )

        // Search input changed
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleSearchInputChanged(_:)),
            name: .searchInputChangedFromNative,
            object: nil
        )
    }
    
    // MARK: - Event Handlers
    
    @objc private func handleTabOpened(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let tabId = userInfo[TabEventContract.tabId] as? String,
              let title = userInfo[TabEventContract.title] as? String else {
            logger.warning("[TabEventEmitter] Invalid tabOpened notification")
            return
        }

        var event: [String: Any] = [
            TabEventContract.tabId: tabId,
            TabEventContract.title: title,
            "systemTabType": userInfo["systemTabType"] as? String ?? NSNull(),
            "timestamp": Date().timeIntervalSince1970 * 1000
        ]

        // Add icon and appUrl if available
        if let icon = userInfo[TabEventContract.icon] as? String {
            event[TabEventContract.icon] = icon
        } else {
            event[TabEventContract.icon] = NSNull()
        }

        if let appUrl = userInfo[TabEventContract.appUrl] as? String {
            event[TabEventContract.appUrl] = appUrl
        } else {
            event[TabEventContract.appUrl] = NSNull()
        }

        logger.debug("[TabEventEmitter] Sending onTabOpened: \(tabId)")
        sendEvent(withName: "onTabOpened", body: event)
    }
    
    @objc private func handleTabClosed(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let tabId = userInfo[TabEventContract.tabId] as? String else {
            logger.warning("[TabEventEmitter] Invalid tabClosed notification")
            return
        }
        
        let event: [String: Any] = [
            TabEventContract.tabId: tabId,
            "timestamp": Date().timeIntervalSince1970 * 1000
        ]
        
        logger.debug("[TabEventEmitter] Sending onTabClosed: \(tabId)")
        sendEvent(withName: "onTabClosed", body: event)
    }
    
    @objc private func handleTabSwitched(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let tabId = userInfo[TabEventContract.tabId] as? String else {
            logger.warning("[TabEventEmitter] Invalid tabSwitched notification")
            return
        }
        
        let event: [String: Any] = [
            TabEventContract.tabId: tabId,
            "previousTabId": userInfo["previousTabId"] as? String ?? NSNull(),
            "timestamp": Date().timeIntervalSince1970 * 1000
        ]
        
        logger.debug("[TabEventEmitter] Sending onTabSwitched: \(tabId)")
        sendEvent(withName: "onTabSwitched", body: event)
    }
    
    @objc private func handleTabUpdated(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let tabId = userInfo[TabEventContract.tabId] as? String else {
            logger.warning("[TabEventEmitter] Invalid tabUpdated notification")
            return
        }

        var event: [String: Any] = [
            TabEventContract.tabId: tabId,
            TabEventContract.title: userInfo[TabEventContract.title] as? String ?? NSNull(),
            "timestamp": Date().timeIntervalSince1970 * 1000
        ]

        // Add icon and appUrl if available
        if let icon = userInfo[TabEventContract.icon] as? String {
            event[TabEventContract.icon] = icon
        }
        if let appUrl = userInfo[TabEventContract.appUrl] as? String {
            event[TabEventContract.appUrl] = appUrl
        }

        logger.debug("[TabEventEmitter] Sending onTabUpdated: \(tabId)")
        sendEvent(withName: "onTabUpdated", body: event)
    }
    
    @objc private func handleTabsReordered(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let tabIds = userInfo["tabIds"] as? [String] else {
            logger.warning("[TabEventEmitter] Invalid tabsReordered notification")
            return
        }
        
        let event: [String: Any] = [
            "tabIds": tabIds,
            "timestamp": Date().timeIntervalSince1970 * 1000
        ]
        
        logger.debug("[TabEventEmitter] Sending onTabsReordered")
        sendEvent(withName: "onTabsReordered", body: event)
    }
    
    @objc private func handleTabEnteredFullscreen(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let tabId = userInfo[TabEventContract.tabId] as? String else {
            logger.warning("[TabEventEmitter] Invalid tabEnteredFullscreen notification")
            return
        }
        let title = userInfo[TabEventContract.title] as? String
        var event = Tab.runtimeFullscreenUserInfo(tabId: tabId, title: title)
        if title == nil {
            event[TabEventContract.title] = NSNull()
        }
        event["timestamp"] = Date().timeIntervalSince1970 * 1000
        
        logger.debug("[TabEventEmitter] Sending onTabEnteredFullscreen: \(tabId)")
        sendEvent(withName: TabEventContract.enteredFullscreen, body: event)
    }
    
    @objc private func handleTabExitedFullscreen(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let tabId = userInfo[TabEventContract.tabId] as? String else {
            logger.warning("[TabEventEmitter] Invalid tabExitedFullscreen notification")
            return
        }
        var event = Tab.runtimeFullscreenUserInfo(tabId: tabId)
        event["timestamp"] = Date().timeIntervalSince1970 * 1000
        
        logger.debug("[TabEventEmitter] Sending onTabExitedFullscreen: \(tabId)")
        sendEvent(withName: TabEventContract.exitedFullscreen, body: event)
    }
    
    @objc private func handleTabBookmarkChanged(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let tabId = userInfo[TabEventContract.tabId] as? String,
              let isBookmarked = userInfo[TabEventContract.isBookmarked] as? Bool else {
            logger.warning("[TabEventEmitter] Invalid tabBookmarkChanged notification")
            return
        }
        var event = Tab.runtimeBookmarkChangedUserInfo(
            tabId: tabId,
            isBookmarked: isBookmarked
        )
        event["timestamp"] = Date().timeIntervalSince1970 * 1000

        logger.debug("[TabEventEmitter] Sending onTabBookmarkChanged: \(tabId), isBookmarked: \(isBookmarked)")
        sendEvent(withName: TabEventContract.bookmarkChanged, body: event)
    }

    @objc private func handleSearchInputChanged(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let inputText = userInfo["inputText"] as? String else {
            logger.warning("[TabEventEmitter] Invalid searchInputChanged notification")
            return
        }

        let event: [String: Any] = [
            "inputText": inputText,
            "timestamp": Date().timeIntervalSince1970 * 1000
        ]

        logger.debug("[TabEventEmitter] Sending onSearchInputChanged: \(inputText)")
        sendEvent(withName: TabEventContract.searchInputChanged, body: event)
    }
}

// MARK: - Notification Names

extension Notification.Name {
    static let tabOpenedFromNative = Notification.Name("tabOpenedFromNative")
    static let tabClosedFromNative = Notification.Name("tabClosedFromNative")
    static let tabSwitchedFromNative = Notification.Name("tabSwitchedFromNative")
    static let tabUpdatedFromNative = Notification.Name("tabUpdatedFromNative")
    static let tabsReorderedFromNative = Notification.Name("tabsReorderedFromNative")
    static let tabEnteredFullscreenFromNative = Notification.Name("tabEnteredFullscreenFromNative")
    static let tabExitedFullscreenFromNative = Notification.Name("tabExitedFullscreenFromNative")
    static let tabBookmarkChangedFromNative = Notification.Name("tabBookmarkChangedFromNative")
    static let searchInputChangedFromNative = Notification.Name("searchInputChangedFromNative")
}
