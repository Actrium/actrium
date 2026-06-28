import SwiftUI
import UIKit
import React
import React_RCTAppDelegate

private let logger = AppLogger.reactNative

struct ReactNativeTabHostView: View {
    @ObservedObject var viewModel: BrowserViewModel
    var historyVM: HistoryViewModel?
    let isLeftPanelVisible: Bool
    let isRightPanelVisible: Bool
    let containerSize: CGSize  // 仅用于调试日志和初始 frame 提示
    var onCoordinatorReady: ((ReactNativeRootRepresentable.Coordinator) -> Void)? = nil

    var body: some View {
        ReactNativeRootRepresentable(
            viewModel: viewModel,
            historyVM: historyVM,
            tabs: viewModel.tabs,
            currentTabId: viewModel.currentTabId,
            isLeftPanelVisible: isLeftPanelVisible,
            isRightPanelVisible: isRightPanelVisible,
            containerSize: containerSize,  // 🔴 直接使用传入的尺寸
            onCoordinatorReady: onCoordinatorReady
        )
    }
}

struct ReactNativeRootRepresentable: UIViewRepresentable {
    @ObservedObject var viewModel: BrowserViewModel
    var historyVM: HistoryViewModel?
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

    func makeUIView(context: Context) -> UIView {
        #if canImport(React)
        logger.notice("[makeUIView] Creating React Native RootView")
        logger.debug("[makeUIView] containerSize: \(containerSize)")
        logger.debug("[makeUIView] tabs.count: \(tabs.count)")
        logger.debug("[makeUIView] currentTabId: \(currentTabId?.uuidString ?? "nil")")
        let properties = makeProperties()
        logger.debug("[makeUIView] properties keys: \(properties.keys.joined(separator: ", "))")
        logger.debug("[makeUIView] calling CanvasReactNativeHost.shared.createRootView")

        let rootView = CanvasReactNativeHost.shared.createRootView(initialProperties: properties)
        logger.debug("[makeUIView] RootView created, bounds: \(rootView.bounds.size)")

        if let rctRootView = rootView as? RCTRootView {
            context.coordinator.rootView = rctRootView
            logger.debug("[makeUIView] stored RCTRootView on coordinator")
        }

        // 交给 SwiftUI 来控制最终尺寸，这里只做一些合理的默认设置
        rootView.translatesAutoresizingMaskIntoConstraints = true
        rootView.autoresizingMask = [.flexibleWidth, .flexibleHeight]

        // 移除了绿色调试背景，让 React Native 内容透明显示
        rootView.backgroundColor = .clear

        // 如果 GeometryReader 已经给出了明确尺寸，可以先设置一个初始 frame（仅用于调试）
        if containerSize.width > 0 && containerSize.height > 0 {
            let rect = CGRect(origin: .zero, size: containerSize)
            rootView.frame = rect
            // 强制设置 bounds
            rootView.bounds = rect
            logger.debug("[makeUIView] preset RootView frame/bounds: \(containerSize)")
        }

        return rootView
        #else
        return UIView()
        #endif
    }

    func updateUIView(_ uiView: UIView, context: Context) {
        #if canImport(React)
        logger.trace("[updateUIView] uiView.bounds: \(uiView.bounds.size), containerSize: \(containerSize)")

        // 确保 UIView 使用自动拉伸，以适配 SwiftUI 提供的 frame
        uiView.translatesAutoresizingMaskIntoConstraints = true
        uiView.autoresizingMask = [.flexibleWidth, .flexibleHeight]

        let properties = makeProperties()

        let newTabId = currentTabId
        context.coordinator.previousTabId = newTabId

        // uiView 本身就是 RootView（或其子类）
        if let rootView = uiView as? RCTRootView {
            if context.coordinator.rootView == nil {
                context.coordinator.rootView = rootView
            }

            logger.trace("[updateUIView] updating RCTRootView.appProperties directly")
            rootView.appProperties = properties

            // 若 GeometryReader 提供了明确的 containerSize，也同步更新一次 frame，方便调试尺寸问题
            if containerSize.width > 0 && containerSize.height > 0 {
                if rootView.frame.size != containerSize {
                    rootView.frame.size = containerSize
                    logger.trace("[updateUIView] RootView frame updated: \(containerSize)")
                }
            }
        } else if let storedRoot = context.coordinator.rootView {
            logger.trace("[updateUIView] updating props via coordinator.rootView")
            storedRoot.appProperties = properties
        }
        #endif
    }

    private func makeProperties() -> [String: Any] {
        let tabsArray: [[String: Any]] = tabs.map { tab in
            var dict: [String: Any] = [
                "id": tab.id.uuidString,
                "title": tab.title,
                "backgroundColor": tab.backgroundColor,
                "status": tab.status.rawValue
            ]

            // Add icon and appUrl if available
            if let icon = tab.icon {
                dict["icon"] = icon
            }

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
                    break
                case .login:
                    // Login tab currently doesn't need extra data from native,
                    // context info (clientId) is handled via JS store using tabId.
                    break
                case .settings:
                    // Settings tab currently doesn't need extra data from native
                    break
                case .globalSearch:
                    // Global search tab currently doesn't need extra data from native
                    break
                }
                return dict
            }

            // Regular ACTR tabs
            if let url = tab.url {
                dict["appUrl"] = url.absoluteString
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

            if let additionalProps = tab.additionalProps {
                for (key, value) in additionalProps {
                    dict[key] = value
                }
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

    @MainActor
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
