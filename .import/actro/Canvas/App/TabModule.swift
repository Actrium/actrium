import Foundation
import React

private let logger = AppLogger.reactNative

@objc(TabModule)
class TabModule: NSObject {

    @objc
    func openTab(_ options: NSDictionary, resolve: @escaping RCTPromiseResolveBlock, reject: @escaping RCTPromiseRejectBlock) {
        logger.notice("[TabModule] openTab called with options: \(options)")
        
        // 如果提供了 systemAppType，打开系统 tab
        if let systemAppType = options["systemAppType"] as? String {
            var additionalProps = [String: Any]()
            for (key, value) in options {
                if let keyString = key as? String, keyString != "tabId" && keyString != "systemAppType" {
                    additionalProps[keyString] = value
                }
            }
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
                        var userInfo: [String: Any] = ["systemAppType": systemAppType]
                        if !additionalProps.isEmpty {
                            userInfo["additionalProps"] = additionalProps
                        }
                        NotificationCenter.default.post(
                            name: .openTabFromRN,
                            object: nil,
                            userInfo: userInfo
                        )
                    } else {
                        viewModel.currentSystemTabType = systemTabType
                    }
                    resolve(tab.id.uuidString)
                    return
                }

                var userInfo: [String: Any] = ["systemAppType": systemAppType]
                if !additionalProps.isEmpty {
                    userInfo["additionalProps"] = additionalProps
                }
                NotificationCenter.default.post(
                    name: .openTabFromRN,
                    object: nil,
                    userInfo: userInfo
                )
                
                #if os(iOS)
                if UIDevice.current.userInterfaceIdiom == .phone {
                    NotificationCenter.default.post(
                        name: .switchPhoneTabFromRN,
                        object: nil,
                        userInfo: ["systemAppType": systemAppType]
                    )
                }
                #endif
                resolve("")
            }
            return
        }
        
        // 否则打开/创建普通 tab
        DispatchQueue.main.async {
            guard let viewModel = BrowserViewModel.active else {
                reject("NO_VIEW_MODEL", "BrowserViewModel not available", nil)
                return
            }
            
            let tabId = options["tabId"] as? String
            var targetTab: Tab?
            
            // 提取额外参数（排除 tabId 和 systemAppType）
            var additionalProps = [String: Any]()
            for (key, value) in options {
                if let keyString = key as? String, keyString != "tabId" && keyString != "systemAppType" {
                    additionalProps[keyString] = value
                }
            }
            
            // 如果提供了 tabId，尝试查找对应的 tab
            if let tabId = tabId, !tabId.isEmpty,
               let uuid = UUID(uuidString: tabId) {
                targetTab = viewModel.tabs.first(where: { $0.id == uuid })
                // 更新已存在 tab 的额外参数
                if var tab = targetTab, targetTab != nil {
                    tab.additionalProps = additionalProps.isEmpty ? nil : additionalProps
                    if let index = viewModel.tabs.firstIndex(where: { $0.id == tab.id }) {
                        viewModel.tabs[index] = tab
                    }
                    targetTab = tab
                }
            }
            
            // 如果没有找到 tab（或 tabId 为空），创建新 tab
            if targetTab == nil {
                targetTab = viewModel.addNewTab()
                logger.info("[TabModule] Created new tab: \(targetTab!.id.uuidString)")
                // 设置新 tab 的额外参数
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
            
            // 切换到目标 tab
            viewModel.currentSystemTabType = nil
            viewModel.currentTabId = tab.id
            viewModel.canvasMode = .focus(tabId: tab.id)
            
            #if os(iOS)
            if UIDevice.current.userInterfaceIdiom == .phone {
                // Phone 端：发送通知进入全屏
                NotificationCenter.default.post(
                    name: .openTabFullscreenFromRN,
                    object: nil,
                    userInfo: ["tabId": tab.id.uuidString]
                )
            }
            #endif
            
            resolve(["tabId": tab.id.uuidString])
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
    func exitFullscreen(_ resolve: @escaping RCTPromiseResolveBlock, reject: @escaping RCTPromiseRejectBlock) {
        logger.notice("[TabModule] exitFullscreen called")

        #if os(iOS)
        DispatchQueue.main.async {
            if UIDevice.current.userInterfaceIdiom == .phone {
                // iPhone：发送通知，让 PhoneMainView 退出全屏
                NotificationCenter.default.post(
                    name: .exitFullscreenFromRN,
                    object: nil
                )
            } else {
                // iPad：通过 viewModel 切换到 Overview 模式
                guard let viewModel = BrowserViewModel.active else {
                    reject("NO_VIEW_MODEL", "BrowserViewModel not available", nil)
                    return
                }
                viewModel.canvasMode = .overview
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
            // 发送通知，让 PhoneMainView 退出搜索页
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
        
        DispatchQueue.main.async {
            guard let viewModel = BrowserViewModel.active else {
                reject("NO_VIEW_MODEL", "BrowserViewModel not available", nil)
                return
            }
            
            guard let uuid = UUID(uuidString: tabId) else {
                reject("INVALID_TAB_ID", "Invalid tab ID format", nil)
                return
            }
            
            // 查找并更新 tab
            if let index = viewModel.tabs.firstIndex(where: { $0.id == uuid }) {
                viewModel.tabs[index].hasUnreadMessage = hasUnread
                viewModel.scheduleSaveTabs()
                resolve(true)
            } else {
                reject("TAB_NOT_FOUND", "Tab with ID \(tabId) not found", nil)
            }
        }
    }
    
    @objc
    func setTabBookmarkStatus(_ tabId: String, isBookmarked: Bool, resolve: @escaping RCTPromiseResolveBlock, reject: @escaping RCTPromiseRejectBlock) {
        logger.notice("[TabModule] setTabBookmarkStatus called for tabId: \(tabId), isBookmarked: \(isBookmarked)")
        
        DispatchQueue.main.async {
            guard let viewModel = BrowserViewModel.active else {
                reject("NO_VIEW_MODEL", "BrowserViewModel not available", nil)
                return
            }
            
            guard let uuid = UUID(uuidString: tabId) else {
                reject("INVALID_TAB_ID", "Invalid tab ID format", nil)
                return
            }
            
            // 查找并更新 tab
            if let index = viewModel.tabs.firstIndex(where: { $0.id == uuid }) {
                viewModel.tabs[index].isBookmarked = isBookmarked
                viewModel.scheduleSaveTabs()
                resolve(true)
            } else {
                reject("TAB_NOT_FOUND", "Tab with ID \(tabId) not found", nil)
            }
        }
    }
    
    @objc
    func updateTabInfo(_ options: NSDictionary, resolve: @escaping RCTPromiseResolveBlock, reject: @escaping RCTPromiseRejectBlock) {
        logger.notice("[TabModule] updateTabInfo called with options: \(options)")
        
        guard let tabId = options["tabId"] as? String else {
            reject("MISSING_TAB_ID", "tabId is required", nil)
            return
        }
        
        DispatchQueue.main.async {
            guard let viewModel = BrowserViewModel.active else {
                reject("NO_VIEW_MODEL", "BrowserViewModel not available", nil)
                return
            }
            
            guard let uuid = UUID(uuidString: tabId) else {
                reject("INVALID_TAB_ID", "Invalid tab ID format", nil)
                return
            }
            
            // 查找并更新 tab
            if let index = viewModel.tabs.firstIndex(where: { $0.id == uuid }) {
                var updatedTab = viewModel.tabs[index]
                if let icon = options["icon"] as? String {
                    updatedTab.icon = icon
                }
                if let title = options["title"] as? String {
                    updatedTab.title = title
                }
                if let urlString = options["appUrl"] as? String, let newURL = URL(string: urlString) {
                    updatedTab.url = newURL
                }
                if let statusString = options["status"] as? String, let status = TabStatus(rawValue: statusString) {
                    updatedTab.status = status
                }
                viewModel.tabs[index] = updatedTab
                resolve(true)
            } else {
                reject("TAB_NOT_FOUND", "Tab with ID \(tabId) not found", nil)
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

            // 返回当前全屏 tab 的 ID（不包括系统 tab）
            if let fullscreenTabId = viewModel.fullscreenTabId {
                resolve(fullscreenTabId.uuidString)
            } else {
                resolve("") // 没有全屏 tab 时返回空字符串
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
                    "tabId": tab.id.uuidString,
                    "title": tab.title,
                    "status": tab.status.rawValue
                ]
                if let icon = tab.icon {
                    tabDict["icon"] = icon
                }
                if let appUrl = tab.url?.absoluteString {
                    tabDict["appUrl"] = appUrl
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
