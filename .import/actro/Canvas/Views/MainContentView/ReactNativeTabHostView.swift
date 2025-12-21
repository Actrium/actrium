import SwiftUI
import AppKit
import React
import React_RCTAppDelegate

private let logger = AppLogger.reactNative

struct ReactNativeTabHostView: View {
    @ObservedObject var viewModel: BrowserViewModel
    var historyVM: HistoryViewModel?
    var bookmarkVM: BookmarkViewModel?
    let isLeftPanelVisible: Bool
    let isRightPanelVisible: Bool
    let containerSize: CGSize  // 仅用于调试日志和初始 frame 提示
    var onCoordinatorReady: ((ReactNativeRootRepresentable.Coordinator) -> Void)? = nil

    var body: some View {
        ReactNativeRootRepresentable(
            viewModel: viewModel,
            historyVM: historyVM,
            bookmarkVM: bookmarkVM,
            tabs: viewModel.tabs,
            currentTabId: viewModel.currentTabId,
            isLeftPanelVisible: isLeftPanelVisible,
            isRightPanelVisible: isRightPanelVisible,
            containerSize: containerSize,  // 🔴 直接使用传入的尺寸
            onCoordinatorReady: onCoordinatorReady
        )
    }
}

struct ReactNativeRootRepresentable: NSViewRepresentable {
    @ObservedObject var viewModel: BrowserViewModel
    var historyVM: HistoryViewModel?
    var bookmarkVM: BookmarkViewModel?
    let tabs: [Tab]
    let currentTabId: UUID?
    let isLeftPanelVisible: Bool
    let isRightPanelVisible: Bool
    let containerSize: CGSize
    var onCoordinatorReady: ((Coordinator) -> Void)? = nil

    func makeCoordinator() -> Coordinator {
        let coordinator = Coordinator(viewModel: viewModel)
        onCoordinatorReady?(coordinator)
        return coordinator
    }

    func makeNSView(context: Context) -> NSView {
        #if canImport(React)
        logger.notice("[makeNSView] Creating React Native RootView")
        logger.debug("[makeNSView] containerSize: \(containerSize)")
        logger.debug("[makeNSView] tabs.count: \(tabs.count)")
        logger.debug("[makeNSView] currentTabId: \(currentTabId?.uuidString ?? "nil")")
        let properties = makeProperties()
        logger.debug("[makeNSView] properties keys: \(properties.keys.joined(separator: ", "))")
        logger.debug("[makeNSView] calling CanvasReactNativeHost.shared.createRootView")

        let rootView = CanvasReactNativeHost.shared.createRootView(initialProperties: properties)
        logger.debug("[makeNSView] RootView created, bounds: \(rootView.bounds.size)")

        if let rctRootView = rootView as? RCTRootView {
            context.coordinator.rootView = rctRootView
            logger.debug("[makeNSView] stored RCTRootView on coordinator")
        }

        // 交给 SwiftUI 来控制最终尺寸，这里只做一些合理的默认设置
        rootView.translatesAutoresizingMaskIntoConstraints = true
        rootView.autoresizingMask = [.width, .height]
        rootView.wantsLayer = true

        // 移除了绿色调试背景，让 React Native 内容透明显示
        rootView.layer?.backgroundColor = NSColor.clear.cgColor

        // 如果 GeometryReader 已经给出了明确尺寸，可以先设置一个初始 frame（仅用于调试）
        if containerSize.width > 0 && containerSize.height > 0 {
            let rect = CGRect(origin: .zero, size: containerSize)
            rootView.frame = rect
            // 强制设置 bounds
            rootView.bounds = rect
            logger.debug("[makeNSView] preset RootView frame/bounds: \(containerSize)")
        }

        return rootView
        #else
        return NSView()
        #endif
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        #if canImport(React)
        logger.trace("[updateNSView] nsView.bounds: \(nsView.bounds.size), containerSize: \(containerSize)")

        // 确保 NSView 使用自动拉伸，以适配 SwiftUI 提供的 frame
        nsView.translatesAutoresizingMaskIntoConstraints = true
        nsView.autoresizingMask = [.width, .height]

        let properties = makeProperties()

        let newTabId = currentTabId
        context.coordinator.previousTabId = newTabId

        // nsView 本身就是 RootView（或其子类）
        if let rootView = nsView as? RCTRootView {
            if context.coordinator.rootView == nil {
                context.coordinator.rootView = rootView
            }

            logger.trace("[updateNSView] updating RCTRootView.appProperties directly")
            rootView.appProperties = properties

            // 若 GeometryReader 提供了明确的 containerSize，也同步更新一次 frame，方便调试尺寸问题
            if containerSize.width > 0 && containerSize.height > 0 {
                if rootView.frame.size != containerSize {
                    rootView.setFrameSize(containerSize)
                    logger.trace("[updateNSView] RootView frame updated: \(containerSize)")
                }
            }
        } else if let storedRoot = context.coordinator.rootView {
            logger.trace("[updateNSView] updating props via coordinator.rootView")
            storedRoot.appProperties = properties
        }
        #endif
    }

    private func makeProperties() -> [String: Any] {
        let tabsArray: [[String: Any]] = tabs.map { tab in
            var dict: [String: Any] = [
                "id": tab.id.uuidString,
                "title": tab.title,
                "backgroundColor": tab.backgroundColor
            ]
            logger.debug("[props] Tab: \(tab.title), backgroundColor: \(tab.backgroundColor)")

            // System app tabs (like chrome:// pages)
            if let systemType = tab.systemTabType {
                dict["systemAppType"] = systemType.rawValue
                logger.debug("[props] System tab: \(systemType.rawValue)")

                // Pass data for system apps
                switch systemType {
                case .history:
                    if let historyVM = historyVM {
                        dict["historyItems"] = historyVM.allItems.map { item in
                            var itemDict: [String: Any] = [
                                "id": item.id.uuidString,
                                "title": item.title,
                                "visitTime": item.visitTime.timeIntervalSince1970 * 1000
                            ]
                            if let url = item.appURL {
                                itemDict["appURL"] = url.absoluteString
                            }
                            return itemDict
                        }
                        dict["historyGroups"] = historyVM.closedGroups.map { group in
                            [
                                "id": group.id.uuidString,
                                "name": group.name,
                                "color": group.color.rawValue,
                                "tabs": group.tabs.map { tab in
                                    var tabDict: [String: Any] = ["title": tab.title]
                                    if let url = tab.appURL {
                                        tabDict["appURL"] = url.absoluteString
                                    }
                                    return tabDict
                                },
                                "closedAt": group.closedAt.timeIntervalSince1970 * 1000
                            ] as [String: Any]
                        }
                    }
                case .bookmarks:
                    if let bookmarkVM = bookmarkVM {
                        dict["bookmarks"] = bookmarkVM.bookmarks.map { bookmark in
                            var bookmarkDict: [String: Any] = [
                                "id": bookmark.id.uuidString,
                                "title": bookmark.title,
                                "createdAt": bookmark.createdAt.timeIntervalSince1970 * 1000
                            ]
                            if let url = bookmark.appURL {
                                bookmarkDict["appURL"] = url.absoluteString
                            }
                            if let lastUsedAt = bookmark.lastUsedAt {
                                bookmarkDict["lastUsed"] = lastUsedAt.timeIntervalSince1970 * 1000
                            }
                            return bookmarkDict
                        }
                    }
                }
                return dict
            }

            // Regular ACTR tabs
            if let url = tab.url {
                dict["url"] = url.absoluteString
            }
            if let htmlContent = tab.htmlContent {
                dict["htmlContent"] = htmlContent
            }
            if let contractName = tab.contractName, let contractVersion = tab.contractVersion {
                dict["contract"] = [
                    "name": contractName,
                    "version": contractVersion
                ]
            }
            if !tab.permissions.isEmpty {
                dict["permissions"] = tab.permissions
            }

            // v2.0: 优先使用 Single Engine 架构 (unified bundle)
            if let unifiedScript = tab.unifiedScript {
                dict["unifiedScript"] = unifiedScript
                dict["rightPanelDefaultVisible"] = tab.rightPanelDefaultVisible
                logger.debug("[props] unifiedScript length: \(unifiedScript.count), rightPanelDefaultVisible: \(tab.rightPanelDefaultVisible)")
            }
            // Legacy: 双 Engine 架构 (leftPanel + rightPanel)
            else if let leftScript = tab.leftPanelScript {
                dict["leftPanelScript"] = leftScript
                logger.debug("[props] leftPanelScript length: \(leftScript.count)")

                if let rightScript = tab.rightPanelScript {
                    dict["rightPanelScript"] = rightScript
                    dict["rightPanelDefaultVisible"] = tab.rightPanelDefaultVisible
                    logger.debug("[props] rightPanelScript length: \(rightScript.count), rightPanelDefaultVisible: \(tab.rightPanelDefaultVisible)")
                }
            }
            // 向后兼容旧架构 (单一 guestScript)
            else if let guestScript = tab.guestScript {
                dict["guestScript"] = guestScript
                logger.debug("[props] guestScript length: \(guestScript.count)")
            }

            return dict
        }

        var properties: [String: Any] = [
            "tabs": tabsArray,
            "isLeftPanelVisible": isLeftPanelVisible,
            "isRightPanelVisible": isRightPanelVisible
        ]

        if let currentTabId = currentTabId {
            properties["currentTabId"] = currentTabId.uuidString
        }

        return properties
    }

    class Coordinator: NSObject {
        @ObservedObject var viewModel: BrowserViewModel
        #if canImport(React)
        var rootView: RCTRootView?
        #endif
        var previousTabId: UUID?

        init(viewModel: BrowserViewModel) {
            self.viewModel = viewModel
            self.previousTabId = viewModel.currentTabId
        }
    }
}
