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
    let containerSize: CGSize  // For debugging logs and initials only frame Hint
    var onCoordinatorReady: ((ReactNativeRootRepresentable.Coordinator) -> Void)? = nil

    var body: some View {
        ReactNativeRootRepresentable(
            viewModel: viewModel,
            historyVM: historyVM,
            tabs: viewModel.tabs,
            currentTabId: viewModel.currentTabId,
            isLeftPanelVisible: isLeftPanelVisible,
            isRightPanelVisible: isRightPanelVisible,
            containerSize: containerSize,  // 🔴 Use imported dimensions directly
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
        logger.debug("[makeUIView] calling ActroReactNativeHost.shared.createRootView")

        let rootView = ActroReactNativeHost.shared.createRootView(initialProperties: properties)
        logger.debug("[makeUIView] RootView created, bounds: \(rootView.bounds.size)")

        if let rctRootView = rootView as? RCTRootView {
            context.coordinator.rootView = rctRootView
            logger.debug("[makeUIView] stored RCTRootView on coordinator")
        }

        // Here. SwiftUI To control the final size, only reasonable defaults are made here.
        rootView.translatesAutoresizingMaskIntoConstraints = true
        rootView.autoresizingMask = [.flexibleWidth, .flexibleHeight]

        // Removes the green debug background, lets React Native Transparent display of content
        rootView.backgroundColor = .clear

        // If GeometryReader A clear size has been given, so you can set an initial. frame(for debugging only)
        if containerSize.width > 0 && containerSize.height > 0 {
            let rect = CGRect(origin: .zero, size: containerSize)
            rootView.frame = rect
            // Force Settings bounds
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

        // Ensure UIView Use automatic stretch to fit SwiftUI Provided frame
        uiView.translatesAutoresizingMaskIntoConstraints = true
        uiView.autoresizingMask = [.flexibleWidth, .flexibleHeight]

        let properties = makeProperties()

        let newTabId = currentTabId
        context.coordinator.previousTabId = newTabId

        // uiView It's all about you. RootView(or subcategory thereof)
        if let rootView = uiView as? RCTRootView {
            if context.coordinator.rootView == nil {
                context.coordinator.rootView = rootView
            }

            logger.trace("[updateUIView] updating RCTRootView.appProperties directly")
            rootView.appProperties = properties

            // If GeometryReader It's clear. containerSize, also to update it simultaneously frame, to debug the size problem.
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
            var dict = tab.runtimeTabInfo.reactNativeInitialProperties
            dict[RuntimeBridgeContract.status] = tab.status.rawValue

            // Add icon and appUrl if available
            if let icon = tab.icon {
                dict[RuntimeBridgeContract.icon] = icon
            }

            logger.debug("[props] Tab: \(tab.title), backgroundColor: \(tab.backgroundColor)")

            // System app tabs (like chrome:// pages)
            if let systemType = tab.systemTabType {
                dict[RuntimeBridgeContract.systemAppType] = systemType.rawValue
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
                dict[RuntimeBridgeContract.appUrl] = url.absoluteString
            }
            if let htmlContent = tab.htmlContent {
                dict[RuntimeTabPropsContract.htmlContent] = htmlContent
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

            // v2.0: Use Priority Single Engine Structure (unified bundle)
            if let unifiedScript = tab.unifiedScript {
                dict[RuntimeTabPropsContract.unifiedScript] = unifiedScript
                dict[RuntimeTabPropsContract.rightPanelDefaultVisible] = tab.rightPanelDefaultVisible
                logger.debug("[props] unifiedScript length: \(unifiedScript.count), rightPanelDefaultVisible: \(tab.rightPanelDefaultVisible)")
            }
            // Legacy: Double Engine Structure (leftPanel + rightPanel)
            else if let leftScript = tab.leftPanelScript {
                dict["leftPanelScript"] = leftScript
                logger.debug("[props] leftPanelScript length: \(leftScript.count)")

                if let rightScript = tab.rightPanelScript {
                    dict["rightPanelScript"] = rightScript
                    dict[RuntimeTabPropsContract.rightPanelDefaultVisible] = tab.rightPanelDefaultVisible
                    logger.debug("[props] rightPanelScript length: \(rightScript.count), rightPanelDefaultVisible: \(tab.rightPanelDefaultVisible)")
                }
            }
            // Backwards compatible with old structures (Single guestScript)
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
            RuntimeRootPropsContract.tabs: tabsArray,
            RuntimeRootPropsContract.isLeftPanelVisible: isLeftPanelVisible,
            RuntimeRootPropsContract.isRightPanelVisible: isRightPanelVisible
        ]

        if let currentTabId = currentTabId {
            properties[RuntimeRootPropsContract.currentTabId] = currentTabId.uuidString
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
