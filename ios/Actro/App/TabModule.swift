import Foundation
import React

private let logger = AppLogger.reactNative

private enum TabModuleBridgeContract {
    static let tabId = RuntimeBridgeContract.tabId
    static let title = RuntimeBridgeContract.title
    static let icon = RuntimeBridgeContract.icon
    static let appUrl = RuntimeBridgeContract.appUrl
    static let status = RuntimeBridgeContract.status
    static let systemAppType = RuntimeBridgeContract.systemAppType
    static let additionalProps = "additionalProps"
}

@objc(TabModule)
class TabModule: NSObject {

    @objc
    func openTab(_ options: NSDictionary, resolve: @escaping RCTPromiseResolveBlock, reject: @escaping RCTPromiseRejectBlock) {
        logger.notice("[TabModule] openTab called with options: \(options)")

        let plan = RuntimeOpenTabContract.plan(from: options)
        let additionalProps = RuntimeOpenTabContract.additionalProps(from: options)

        // If you do, systemAppTypeOpen the system. tab
        if plan.kind == RuntimeOpenTabContract.kindSystemPage,
           let systemAppType = plan.systemAppType {
            DispatchQueue.main.async {
                #if os(iOS)
                let isPhone = UIDevice.current.userInterfaceIdiom == .phone
                #else
                let isPhone = false
                #endif

                if !isPhone,
                   let viewModel = BrowserViewModel.active,
                   let systemTabType = SystemTabType(rawValue: systemAppType) {
                    let tab = viewModel.openSystemTab(
                        systemTabType,
                        additionalProps: additionalProps.isEmpty ? nil : additionalProps,
                        replaceAdditionalProps: !additionalProps.isEmpty
                    )
                    if systemTabType == .settings {
                        NotificationCenter.default.post(
                            name: .openTabFromRN,
                            object: nil,
                            userInfo: Self.systemTabUserInfo(
                                systemAppType: systemAppType,
                                additionalProps: additionalProps
                            )
                        )
                    } else {
                        viewModel.currentSystemTabType = systemTabType
                    }
                    resolve(tab.id.uuidString)
                    return
                }

                NotificationCenter.default.post(
                    name: .openTabFromRN,
                    object: nil,
                    userInfo: Self.systemTabUserInfo(
                        systemAppType: systemAppType,
                        additionalProps: additionalProps
                    )
                )
                
                #if os(iOS)
                if UIDevice.current.userInterfaceIdiom == .phone {
                    NotificationCenter.default.post(
                        name: .switchPhoneTabFromRN,
                        object: nil,
                        userInfo: Self.systemTabUserInfo(
                            systemAppType: systemAppType,
                            additionalProps: [:]
                        )
                    )
                }
                #endif
                resolve("")
            }
            return
        }
        
        // Open it or something./Create Normal tab
        DispatchQueue.main.async {
            guard let viewModel = BrowserViewModel.active else {
                reject("NO_VIEW_MODEL", "BrowserViewModel not available", nil)
                return
            }
            
            let tabId = plan.tabId
            var targetTab: Tab?

            // If you do, tabId, try to find the corresponding one tab
            if let tabId = tabId, !tabId.isEmpty,
               let uuid = UUID(uuidString: tabId) {
                targetTab = viewModel.tabs.first(where: { $0.id == uuid })
                // Update Exists tab Additional Parameters
                if var tab = targetTab, targetTab != nil {
                    tab.additionalProps = additionalProps.isEmpty ? nil : additionalProps
                    if let index = viewModel.tabs.firstIndex(where: { $0.id == tab.id }) {
                        viewModel.tabs[index] = tab
                    }
                    targetTab = tab
                }
            }
            
            // If you don't find it, tab(or tabId ) Create new tab
            if targetTab == nil {
                targetTab = viewModel.addNewTab()
                logger.info("[TabModule] Created new tab: \(targetTab!.id.uuidString)")
                // Setup New tab Additional Parameters
                if var tab = targetTab {
                    tab.additionalProps = additionalProps.isEmpty ? nil : additionalProps
                    if let index = viewModel.tabs.firstIndex(where: { $0.id == tab.id }) {
                        viewModel.tabs[index] = tab
                    }
                    targetTab = tab
                }
            }
            
            guard let tab = targetTab else {
                reject("TAB_ERROR", "Failed to create or find tab", nil)
                return
            }
            
            // Switch to Target tab
            viewModel.currentSystemTabType = nil
            viewModel.currentTabId = tab.id
            viewModel.actroMode = .focus(tabId: tab.id)
            
            #if os(iOS)
            if UIDevice.current.userInterfaceIdiom == .phone {
                // Phone End: Send a notice into fullscreen
                NotificationCenter.default.post(
                    name: .openTabFullscreenFromRN,
                    object: nil,
                    userInfo: Self.tabFullscreenUserInfo(tab.id.uuidString)
                )
            }
            #endif
            
            resolve([TabModuleBridgeContract.tabId: tab.id.uuidString])
        }
    }

    @objc
    func selectTab(_ tabId: String, resolve: @escaping RCTPromiseResolveBlock, reject: @escaping RCTPromiseRejectBlock) {
        logger.notice("[TabModule] selectTab called with tabId: \(tabId)")
        
        DispatchQueue.main.async {
            NotificationCenter.default.post(
                name: .selectTabFromRN,
                object: nil,
                userInfo: Self.tabIdUserInfo(tabId)
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
                userInfo: Self.tabIdUserInfo(tabId)
            )
            resolve(true)
        }
    }
    
    @objc
    func exitFullscreen(_ resolve: @escaping RCTPromiseResolveBlock, reject: @escaping RCTPromiseRejectBlock) {
        logger.notice("[TabModule] exitFullscreen called")

        #if os(iOS)
        DispatchQueue.main.async {
            if UIDevice.current.userInterfaceIdiom == .phone {
                // iPhone: Send notice, Jean PhoneMainView Exit Full Screen
                NotificationCenter.default.post(
                    name: .exitFullscreenFromRN,
                    object: nil
                )
            } else {
                // iPad: adopted viewModel Switch to Overview Mode
                guard let viewModel = BrowserViewModel.active else {
                    reject("NO_VIEW_MODEL", "BrowserViewModel not available", nil)
                    return
                }
                viewModel.actroMode = .overview
            }
            resolve(true)
        }
        #else
        reject("UNSUPPORTED_PLATFORM", "This method is only supported on iOS", nil)
        #endif
    }
    
    @objc
    func exitSearch(_ resolve: @escaping RCTPromiseResolveBlock, reject: @escaping RCTPromiseRejectBlock) {
        logger.notice("[TabModule] exitSearch called")
        #if os(iOS)
        DispatchQueue.main.async {
            // Send a notice, Jean. PhoneMainView Quit Search Page
            NotificationCenter.default.post(
                name: .exitSearchFromRN,
                object: nil
            )
            resolve(true)
        }
        #else
        reject("UNSUPPORTED_PLATFORM", "This method is only supported on iOS", nil)
        return
        #endif
    }
    
    @objc
    func setTabUnreadStatus(_ tabId: String, hasUnread: Bool, resolve: @escaping RCTPromiseResolveBlock, reject: @escaping RCTPromiseRejectBlock) {
        logger.notice("[TabModule] setTabUnreadStatus called for tabId: \(tabId), hasUnread: \(hasUnread)")
        guard let payload = RuntimeTabMutationContract.unread(tabId: tabId, unread: hasUnread) else {
            reject("MISSING_PAYLOAD", "tabId and unread are required", nil)
            return
        }
        let payloadTabId = payload.tabId
        let payloadUnread = payload.unread
        
        DispatchQueue.main.async {
            guard let viewModel = BrowserViewModel.active else {
                reject("NO_VIEW_MODEL", "BrowserViewModel not available", nil)
                return
            }
            
            guard let uuid = UUID(uuidString: payloadTabId) else {
                reject("INVALID_TAB_ID", "Invalid tab ID format", nil)
                return
            }
            
            // Find and update tab
            if let index = viewModel.tabs.firstIndex(where: { $0.id == uuid }) {
                viewModel.tabs[index].hasUnreadMessage = payloadUnread
                viewModel.scheduleSaveTabs()
                resolve(true)
            } else {
                reject("TAB_NOT_FOUND", "Tab with ID \(payloadTabId) not found", nil)
            }
        }
    }
    
    @objc
    func setTabBookmarkStatus(_ tabId: String, isBookmarked: Bool, resolve: @escaping RCTPromiseResolveBlock, reject: @escaping RCTPromiseRejectBlock) {
        logger.notice("[TabModule] setTabBookmarkStatus called for tabId: \(tabId), isBookmarked: \(isBookmarked)")
        guard let payload = RuntimeTabMutationContract.bookmarkStatus(tabId: tabId, isBookmarked: isBookmarked) else {
            reject("MISSING_PAYLOAD", "tabId and isBookmarked are required", nil)
            return
        }
        let payloadTabId = payload.tabId
        let payloadMarked = payload.marked
        
        DispatchQueue.main.async {
            guard let viewModel = BrowserViewModel.active else {
                reject("NO_VIEW_MODEL", "BrowserViewModel not available", nil)
                return
            }
            
            guard let uuid = UUID(uuidString: payloadTabId) else {
                reject("INVALID_TAB_ID", "Invalid tab ID format", nil)
                return
            }
            
            // Find and update tab
            if let index = viewModel.tabs.firstIndex(where: { $0.id == uuid }) {
                viewModel.tabs[index].isBookmarked = payloadMarked
                viewModel.scheduleSaveTabs()
                resolve(true)
            } else {
                reject("TAB_NOT_FOUND", "Tab with ID \(payloadTabId) not found", nil)
            }
        }
    }
    
    @objc
    func updateTabInfo(_ options: NSDictionary, resolve: @escaping RCTPromiseResolveBlock, reject: @escaping RCTPromiseRejectBlock) {
        logger.notice("[TabModule] updateTabInfo called with options: \(options)")

        guard let update = RuntimeTabMutationContract.update(from: options) else {
            reject("MISSING_TAB_ID", "tabId is required", nil)
            return
        }
        let updateTabId = update.tabId
        let updateTitle = update.title
        let updateIcon = update.icon
        let updateAppUrl = update.appUrl
        let updateStatus = update.status
        
        DispatchQueue.main.async {
            guard let viewModel = BrowserViewModel.active else {
                reject("NO_VIEW_MODEL", "BrowserViewModel not available", nil)
                return
            }
            
            guard let uuid = UUID(uuidString: updateTabId) else {
                reject("INVALID_TAB_ID", "Invalid tab ID format", nil)
                return
            }
            
            // Find and update tab
            if let index = viewModel.tabs.firstIndex(where: { $0.id == uuid }) {
                var updatedTab = viewModel.tabs[index]
                if let icon = updateIcon {
                    updatedTab.icon = icon
                }
                if let title = updateTitle {
                    updatedTab.title = title
                }
                if let urlString = updateAppUrl, let newURL = URL(string: urlString) {
                    updatedTab.url = newURL
                }
                if let status = updateStatus {
                    updatedTab.status = status
                }
                viewModel.tabs[index] = updatedTab
                resolve(true)
            } else {
                reject("TAB_NOT_FOUND", "Tab with ID \(updateTabId) not found", nil)
            }
        }
    }
    
    @objc
    func getCurrentFullscreenTab(_ resolve: @escaping RCTPromiseResolveBlock, reject: @escaping RCTPromiseRejectBlock) {
        logger.notice("[TabModule] getCurrentFullscreenTab called")

        #if os(iOS)
        DispatchQueue.main.async {
            guard let viewModel = BrowserViewModel.active else {
                reject("NO_VIEW_MODEL", "BrowserViewModel not available", nil)
                return
            }

            // Return the current fullscreen tab It's... ID(excluding systems) tab)
            if let fullscreenTabId = viewModel.fullscreenTabId {
                resolve(fullscreenTabId.uuidString)
            } else {
                resolve("") // No Full Screen tab Return empty string
            }
        }
        #else
        reject("UNSUPPORTED_PLATFORM", "This method is only supported on iOS", nil)
        return
        #endif
    }

    @objc
    func getAllTabs(_ resolve: @escaping RCTPromiseResolveBlock, reject: @escaping RCTPromiseRejectBlock) {
        logger.notice("[TabModule] getAllTabs called")

        DispatchQueue.main.async {
            guard let viewModel = BrowserViewModel.active else {
                reject("NO_VIEW_MODEL", "BrowserViewModel not available", nil)
                return
            }

            let tabsArray: [[String: Any]] = viewModel.tabs.map { tab in
                var tabDict: [String: Any] = [
                    TabModuleBridgeContract.tabId: tab.id.uuidString,
                    TabModuleBridgeContract.title: tab.title,
                    TabModuleBridgeContract.status: tab.status.rawValue
                ]
                if let icon = tab.icon {
                    tabDict[TabModuleBridgeContract.icon] = icon
                }
                if let appUrl = tab.url?.absoluteString {
                    tabDict[TabModuleBridgeContract.appUrl] = appUrl
                }
                return tabDict
            }

            resolve(tabsArray)
        }
    }

    @objc
    static func requiresMainQueueSetup() -> Bool {
        return false
    }

    private static func tabIdUserInfo(_ tabId: String) -> [String: Any] {
        [TabModuleBridgeContract.tabId: tabId]
    }

    private static func systemTabUserInfo(
        systemAppType: String,
        additionalProps: [String: Any]
    ) -> [String: Any] {
        var userInfo: [String: Any] = [
            TabModuleBridgeContract.systemAppType: systemAppType
        ]
        if !additionalProps.isEmpty {
            userInfo[TabModuleBridgeContract.additionalProps] = additionalProps
        }
        return userInfo
    }

    private static func tabFullscreenUserInfo(_ tabId: String) -> [String: Any] {
        Tab.runtimeFullscreenUserInfo(tabId: tabId)
    }
}

extension Notification.Name {
    static let openTabFromRN = Notification.Name("openTabFromRN")
    static let selectTabFromRN = Notification.Name("selectTabFromRN")
    static let closeTabFromRN = Notification.Name("closeTabFromRN")
    static let exitFullscreenFromRN = Notification.Name("exitFullscreenFromRN")
    static let openTabFullscreenFromRN = Notification.Name("openTabFullscreenFromRN")
    static let exitSearchFromRN = Notification.Name("exitSearchFromRN")
    static let switchPhoneTabFromRN = Notification.Name("switchPhoneTabFromRN")
}
