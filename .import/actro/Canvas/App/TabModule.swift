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
                resolve(newTabId.uuidString)
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
            viewModel.currentTabId = tab.id
            
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
        if UIDevice.current.userInterfaceIdiom != .phone {
            reject("UNSUPPORTED_DEVICE", "This method is only supported on iPhone", nil)
            return
        }
        #else
        reject("UNSUPPORTED_PLATFORM", "This method is only supported on iOS", nil)
        return
        #endif

        DispatchQueue.main.async {
            // 发送通知，让 PhoneMainView 退出全屏
            NotificationCenter.default.post(
                name: .exitFullscreenFromRN,
                object: nil
            )
            resolve(true)
        }
    }
    
    @objc
    func exitSearch(_ resolve: @escaping RCTPromiseResolveBlock, reject: @escaping RCTPromiseRejectBlock) {
        logger.notice("[TabModule] exitSearch called")
        
        #if os(iOS)
        if UIDevice.current.userInterfaceIdiom != .phone {
            reject("UNSUPPORTED_DEVICE", "This method is only supported on iPhone", nil)
            return
        }
        #else
        reject("UNSUPPORTED_PLATFORM", "This method is only supported on iOS", nil)
        return
        #endif

        DispatchQueue.main.async {
            // 发送通知，让 PhoneMainView 退出搜索页
            NotificationCenter.default.post(
                name: .exitSearchFromRN,
                object: nil
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
    static let exitFullscreenFromRN = Notification.Name("exitFullscreenFromRN")
    static let openTabFullscreenFromRN = Notification.Name("openTabFullscreenFromRN")
    static let exitSearchFromRN = Notification.Name("exitSearchFromRN")
}
