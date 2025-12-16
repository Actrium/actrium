import SwiftUI
import AppKit

#if canImport(React)
import React
#endif

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
        print("🚀🚀🚀 [makeNSView] ========== 开始创建 React Native RootView ==========")
        print("🔍 [makeNSView] 期望容器尺寸: \(containerSize)")
        print("🔍 [makeNSView] Tabs 数量: \(tabs.count)")
        print("🔍 [makeNSView] 当前 TabId: \(currentTabId?.uuidString ?? "nil")")
        let properties = makeProperties()
        print("🔍 [makeNSView] properties 创建完成，keys: \(properties.keys.joined(separator: ", "))")
        print("🔍 [makeNSView] 准备调用 CanvasReactNativeHost.shared.createRootView")

        let rootView = CanvasReactNativeHost.shared.createRootView(initialProperties: properties)
        print("🔍 [makeNSView] RootView 创建完成，初始 bounds: \(rootView.bounds.size)")

        if let rctRootView = rootView as? RCTRootView {
            context.coordinator.rootView = rctRootView
            print("🔍 [makeNSView] RCTRootView 已保存到 coordinator")
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
            print("✅ [makeNSView] 预设 RootView frame 为: \(containerSize)")
            print("✅ [makeNSView] 预设 RootView bounds 为: \(rootView.bounds.size)")
        }

        return rootView
        #else
        return NSView()
        #endif
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        #if canImport(React)
        print("🔍 [updateNSView] 被调用，当前 nsView.bounds.size: \(nsView.bounds.size)，期望 containerSize: \(containerSize)")

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

            print("🔍 [updateNSView] 直接更新 RCTRootView.appProperties")
            rootView.appProperties = properties

            // 若 GeometryReader 提供了明确的 containerSize，也同步更新一次 frame，方便调试尺寸问题
            if containerSize.width > 0 && containerSize.height > 0 {
                if rootView.frame.size != containerSize {
                    rootView.setFrameSize(containerSize)
                    print("✅ [updateNSView] RootView frame 已更新为: \(containerSize)")
                }
            }
        } else if let storedRoot = context.coordinator.rootView {
            print("🔍 [updateNSView] 使用 coordinator 中保存的 RCTRootView 更新属性")
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
            print("🎨 传递给 RN - Tab: \(tab.title), backgroundColor: \(tab.backgroundColor)")

            // System app tabs (like chrome:// pages)
            if let systemType = tab.systemTabType {
                dict["systemAppType"] = systemType.rawValue
                print("✅ 系统应用 Tab: \(systemType.rawValue)")

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
                print("✅ 传递 unifiedScript 到 RN (Single Engine)，长度: \(unifiedScript.count), 右侧面板默认\(tab.rightPanelDefaultVisible ? "显示" : "隐藏")")
            }
            // Legacy: 双 Engine 架构 (leftPanel + rightPanel)
            else if let leftScript = tab.leftPanelScript {
                dict["leftPanelScript"] = leftScript
                print("✅ 传递 leftPanelScript 到 RN (Legacy)，长度: \(leftScript.count)")

                if let rightScript = tab.rightPanelScript {
                    dict["rightPanelScript"] = rightScript
                    dict["rightPanelDefaultVisible"] = tab.rightPanelDefaultVisible
                    print("✅ 传递 rightPanelScript 到 RN (Legacy)，长度: \(rightScript.count), 默认\(tab.rightPanelDefaultVisible ? "显示" : "隐藏")")
                }
            }
            // 向后兼容旧架构 (单一 guestScript)
            else if let guestScript = tab.guestScript {
                dict["guestScript"] = guestScript
                print("✅ 传递 guestScript 到 RN (兼容模式)，长度: \(guestScript.count)")
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
