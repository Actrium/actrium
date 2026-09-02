import Foundation
import ActriumCore

enum RuntimeBridgeContract {
    static let tabId = runtimeBridgeTabIdKey()
    static let title = runtimeBridgeTitleKey()
    static let status = runtimeBridgeStatusKey()
    static let icon = runtimeBridgeIconKey()
    static let appUrl = runtimeBridgeAppUrlKey()
    static let isBookmarked = runtimeBridgeIsBookmarkedKey()
    static let containerWidth = runtimeBridgeContainerWidthKey()
    static let containerHeight = runtimeBridgeContainerHeightKey()
    static let systemAppType = RuntimeOpenTabContract.systemAppType
    static let tabEnteredFullscreenEvent = runtimeTabEnteredFullscreenEventName()
    static let tabExitedFullscreenEvent = runtimeTabExitedFullscreenEventName()
    static let tabBookmarkChangedEvent = runtimeTabBookmarkChangedEventName()
    static let searchInputChangedEvent = runtimeSearchInputChangedEventName()
}

enum RuntimeTabPropsContract {
    static let unifiedScript = runtimeTabPropsUnifiedScriptKey()
    static let rightPanelDefaultVisible = runtimeTabPropsRightPanelDefaultVisibleKey()
    static let id = runtimeTabPropsIdKey()
    static let backgroundColor = runtimeTabPropsBackgroundColorKey()
    static let reloadKey = runtimeTabPropsReloadKeyKey()
    static let htmlContent = runtimeTabPropsHtmlContentKey()
}

enum RuntimeRootPropsContract {
    static let singleTabMode = runtimeRootPropsSingleTabModeKey()
    static let tab = runtimeRootPropsTabKey()
    static let tabs = runtimeRootPropsTabsKey()
    static let currentTabId = runtimeRootPropsCurrentTabIdKey()
    static let isLeftPanelVisible = runtimeRootPropsIsLeftPanelVisibleKey()
    static let isRightPanelVisible = runtimeRootPropsIsRightPanelVisibleKey()
}

struct RuntimeOpenTabPlan {
    let kind: String
    let tabId: String?
    let systemAppType: String?
}

enum RuntimeOpenTabContract {
    static let kindApp = "app"
    static let kindSystemPage = "systemPage"
    static let kindSystemTab = "systemTab"
    static let tabId = RuntimeBridgeContract.tabId
    static let systemAppType = "systemAppType"
    static let tabSystemAppType = "tabSystemAppType"

    static func plan(
        from options: NSDictionary,
        preferTabletRoutes: Bool = false,
        systemKeyPresenceMatters: Bool = false
    ) -> RuntimeOpenTabPlan {
        let systemAppTypeValue = options[systemAppType] as? String
        let hasSystemAppType = systemKeyPresenceMatters
            ? options.object(forKey: systemAppType) != nil
            : systemAppTypeValue != nil

        let tabIdValue = options[tabId] as? String
        let hasTabSystemAppType = options.object(forKey: tabSystemAppType) != nil
        let kind: String
        if hasSystemAppType {
            kind = kindSystemPage
        } else if preferTabletRoutes && hasTabSystemAppType {
            kind = kindSystemTab
        } else {
            kind = kindApp
        }

        return RuntimeOpenTabPlan(
            kind: kind,
            tabId: tabIdValue,
            systemAppType: systemAppTypeValue
        )
    }

    static func additionalProps(from options: NSDictionary) -> [String: Any] {
        dictionary(from: options, excluding: [tabId, systemAppType])
    }

    private static func dictionary(
        from options: NSDictionary,
        excluding keys: Set<String>
    ) -> [String: Any] {
        var result = [String: Any]()
        for (key, value) in options {
            guard let keyString = key as? String,
                  !keys.contains(keyString) else { continue }
            result[keyString] = value
        }
        return result
    }
}

struct RuntimeTabUnreadPayload {
    let tabId: String
    let unread: Bool
}

struct RuntimeTabBookmarkStatusPayload {
    let tabId: String
    let marked: Bool
}

struct RuntimeTabUpdatePayload {
    let tabId: String
    let title: String?
    let icon: String?
    let appUrl: String?
    let status: TabStatus?
}

enum RuntimeTabMutationContract {
    static func unread(tabId: String, unread: Bool) -> RuntimeTabUnreadPayload? {
        guard let payload = runtimeTabUnreadFromFields(tabId: tabId, unread: unread) else {
            return nil
        }
        return RuntimeTabUnreadPayload(tabId: payload.tabId, unread: payload.unread)
    }

    static func bookmarkStatus(tabId: String, isBookmarked: Bool) -> RuntimeTabBookmarkStatusPayload? {
        guard let payload = runtimeTabBookmarkStatusFromFields(tabId: tabId, marked: isBookmarked) else {
            return nil
        }
        return RuntimeTabBookmarkStatusPayload(tabId: payload.tabId, marked: payload.marked)
    }

    static func update(from options: NSDictionary) -> RuntimeTabUpdatePayload? {
        let status = (options[RuntimeBridgeContract.status] as? String).flatMap {
            normalizeRuntimeAppStatus(value: $0)
        }
        guard let update = runtimeTabUpdateFromFields(
            tabId: options[RuntimeBridgeContract.tabId] as? String,
            title: options[RuntimeBridgeContract.title] as? String,
            icon: options[RuntimeBridgeContract.icon] as? String,
            appUrl: options[RuntimeBridgeContract.appUrl] as? String,
            status: status
        ) else {
            return nil
        }
        return RuntimeTabUpdatePayload(
            tabId: update.tabId,
            title: update.title,
            icon: update.icon,
            appUrl: update.appUrl,
            status: update.status.flatMap(TabStatus.init(rawValue:))
        )
    }
}

// MARK: - Runtime Bridge Tab Adapter

extension Tab {
    var runtimeTabInfo: RuntimeTabInfo {
        RuntimeTabInfo(
            unifiedScript: unifiedScript ?? "",
            rightPanelDefaultVisible: rightPanelDefaultVisible,
            id: id.uuidString,
            backgroundColor: backgroundColor,
            reloadKey: Int32(clamping: reloadNonce),
            htmlContent: htmlContent ?? "",
            title: title
        )
    }

    func runtimeSingleRootProperties(
        isLeftPanelVisible: Bool,
        containerHeight: Int32,
        containerWidth: Int32
    ) -> (tab: [String: Any], root: [String: Any]) {
        let runtimeTab = runtimeTabInfo
        let runtimeProps = runtimeRootPropsFromRuntimeTabs(
            tab: runtimeTab,
            tabs: [runtimeTab],
            isLeftPanelVisible: isLeftPanelVisible,
            isRightPanelVisible: systemTabType == nil,
            containerHeight: containerHeight,
            containerWidth: containerWidth,
            appUrl: url?.absoluteString ?? ""
        )

        var root = runtimeProps.reactNativeInitialProperties
        root.removeValue(forKey: RuntimeRootPropsContract.tabs)
        return (runtimeProps.tab.reactNativeInitialProperties, root)
    }

    func runtimeBookmarkChangedUserInfo(isBookmarked: Bool) -> [String: Any] {
        Self.runtimeBookmarkChangedUserInfo(
            tabId: id.uuidString,
            isBookmarked: isBookmarked
        )
    }

    static func runtimeBookmarkChangedUserInfo(tabId: String, isBookmarked: Bool) -> [String: Any] {
        let payload = runtimeTabBookmarkEvent(
            tabId: tabId,
            isBookmarked: isBookmarked
        )
        return [
            RuntimeBridgeContract.tabId: payload.tabId,
            RuntimeBridgeContract.isBookmarked: payload.isBookmarked
        ]
    }

    var runtimeFullscreenEnteredUserInfo: [String: Any] {
        Self.runtimeFullscreenUserInfo(tabId: id.uuidString, title: title)
    }

    var runtimeFullscreenExitedUserInfo: [String: Any] {
        Self.runtimeFullscreenUserInfo(tabId: id.uuidString)
    }

    static func runtimeFullscreenUserInfo(tabId: String, title: String? = nil) -> [String: Any] {
        let payload = runtimeTabFullscreenEvent(tabId: tabId)
        var userInfo: [String: Any] = [
            RuntimeBridgeContract.tabId: payload.tabId
        ]
        if let title = title {
            userInfo[RuntimeBridgeContract.title] = title
        }
        return userInfo
    }
}

extension RuntimeTabInfo {
    var reactNativeInitialProperties: [String: Any] {
        [
            RuntimeTabPropsContract.unifiedScript: unifiedScript,
            RuntimeTabPropsContract.rightPanelDefaultVisible: rightPanelDefaultVisible,
            RuntimeTabPropsContract.id: id,
            RuntimeTabPropsContract.backgroundColor: backgroundColor,
            RuntimeTabPropsContract.reloadKey: Int(reloadKey),
            RuntimeTabPropsContract.htmlContent: htmlContent,
            RuntimeBridgeContract.title: title
        ]
    }
}

extension RuntimeRootPropsInfo {
    var reactNativeInitialProperties: [String: Any] {
        [
            RuntimeRootPropsContract.singleTabMode: singleTabMode,
            RuntimeRootPropsContract.tab: tab.reactNativeInitialProperties,
            RuntimeRootPropsContract.tabs: tabs.map(\.reactNativeInitialProperties),
            RuntimeRootPropsContract.currentTabId: currentTabId,
            RuntimeRootPropsContract.isLeftPanelVisible: isLeftPanelVisible,
            RuntimeRootPropsContract.isRightPanelVisible: isRightPanelVisible,
            RuntimeBridgeContract.containerWidth: Int(containerWidth),
            RuntimeBridgeContract.containerHeight: Int(containerHeight),
            RuntimeBridgeContract.appUrl: appUrl ?? ""
        ]
    }
}
