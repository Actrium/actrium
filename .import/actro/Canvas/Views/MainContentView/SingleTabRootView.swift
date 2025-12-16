import SwiftUI
import AppKit

#if canImport(React)
import React
#endif

/// Single Tab RootView - 每个 Tab 独立的 React Native RootView
/// 实现一个 Bridge + 多个 RootView 架构
struct SingleTabRootView: View {
    let tab: Tab
    @ObservedObject var viewModel: BrowserViewModel
    var historyVM: HistoryViewModel?
    var bookmarkVM: BookmarkViewModel?
    let containerSize: CGSize

    var body: some View {
        SingleTabRootRepresentable(
            tab: tab,
            viewModel: viewModel,
            historyVM: historyVM,
            bookmarkVM: bookmarkVM,
            containerSize: containerSize
        )
    }
}

struct SingleTabRootRepresentable: NSViewRepresentable {
    let tab: Tab
    @ObservedObject var viewModel: BrowserViewModel
    var historyVM: HistoryViewModel?
    var bookmarkVM: BookmarkViewModel?
    let containerSize: CGSize

    func makeCoordinator() -> Coordinator {
        Coordinator(tab: tab, viewModel: viewModel)
    }

    func makeNSView(context: Context) -> NSView {
        #if canImport(React)
        print("🚀 [SingleTabRootView] 创建 RootView for Tab: \(tab.title) (\(tab.id.uuidString.prefix(8)))")

        let properties = makeProperties()
        let rootView = CanvasReactNativeHost.shared.createRootView(initialProperties: properties)

        if let rctRootView = rootView as? RCTRootView {
            context.coordinator.rootView = rctRootView
        }

        rootView.translatesAutoresizingMaskIntoConstraints = true
        rootView.autoresizingMask = [.width, .height]
        rootView.wantsLayer = true
        rootView.layer?.backgroundColor = NSColor.clear.cgColor

        if containerSize.width > 0 && containerSize.height > 0 {
            rootView.frame = CGRect(origin: .zero, size: containerSize)
            rootView.bounds = CGRect(origin: .zero, size: containerSize)
        }

        return rootView
        #else
        return NSView()
        #endif
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        #if canImport(React)
        let properties = makeProperties()

        if let rootView = nsView as? RCTRootView {
            rootView.appProperties = properties

            if containerSize.width > 0 && containerSize.height > 0 {
                if rootView.frame.size != containerSize {
                    rootView.setFrameSize(containerSize)
                }
            }
        }
        #endif
    }

    private func makeProperties() -> [String: Any] {
        var tabDict: [String: Any] = [
            "id": tab.id.uuidString,
            "title": tab.title,
            "backgroundColor": tab.backgroundColor
        ]

        // System app tabs (History/Bookmarks)
        if let systemType = tab.systemTabType {
            tabDict["systemAppType"] = systemType.rawValue

            switch systemType {
            case .history:
                if let historyVM = historyVM {
                    tabDict["historyItems"] = historyVM.allItems.map { item in
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
                    tabDict["historyGroups"] = historyVM.closedGroups.map { group in
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
                    tabDict["bookmarks"] = bookmarkVM.bookmarks.map { bookmark in
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
        } else {
            // Regular ACTR tabs
            if let url = tab.url {
                tabDict["url"] = url.absoluteString
            }
            if let htmlContent = tab.htmlContent {
                tabDict["htmlContent"] = htmlContent
            }
            if let contractName = tab.contractName, let contractVersion = tab.contractVersion {
                tabDict["contract"] = [
                    "name": contractName,
                    "version": contractVersion
                ]
            }
            if !tab.permissions.isEmpty {
                tabDict["permissions"] = tab.permissions
            }

            // v2.0: Single Engine 架构
            if let unifiedScript = tab.unifiedScript {
                tabDict["unifiedScript"] = unifiedScript
                tabDict["rightPanelDefaultVisible"] = tab.rightPanelDefaultVisible
            }
            // Legacy: 双 Engine 架构
            else if let leftScript = tab.leftPanelScript {
                tabDict["leftPanelScript"] = leftScript
                if let rightScript = tab.rightPanelScript {
                    tabDict["rightPanelScript"] = rightScript
                    tabDict["rightPanelDefaultVisible"] = tab.rightPanelDefaultVisible
                }
            }
            // 兼容旧架构
            else if let guestScript = tab.guestScript {
                tabDict["guestScript"] = guestScript
            }
        }

        // Single tab mode - 只传递当前 tab
        return [
            "singleTabMode": true,
            "tab": tabDict,
            "currentTabId": tab.id.uuidString,
            "isLeftPanelVisible": viewModel.isLeftPanelVisible(for: tab.id),
            "isRightPanelVisible": tab.systemTabType == nil
        ]
    }

    class Coordinator: NSObject {
        let tab: Tab
        @ObservedObject var viewModel: BrowserViewModel
        #if canImport(React)
        var rootView: RCTRootView?
        #endif

        init(tab: Tab, viewModel: BrowserViewModel) {
            self.tab = tab
            self.viewModel = viewModel
        }

        deinit {
            print("🗑️ [SingleTabRootView] Coordinator deinit for Tab: \(tab.title)")
        }
    }
}
