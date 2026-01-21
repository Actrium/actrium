import SwiftUI
import UIKit
import React
import React_RCTAppDelegate

private let logger = AppLogger.reactNative

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

struct SingleTabRootRepresentable: UIViewRepresentable {
    let tab: Tab
    @ObservedObject var viewModel: BrowserViewModel
    var historyVM: HistoryViewModel?
    var bookmarkVM: BookmarkViewModel?
    let containerSize: CGSize

    func makeCoordinator() -> Coordinator {
        Coordinator(tab: tab, viewModel: viewModel)
    }

    func makeUIView(context: Context) -> UIView {
        #if canImport(React)
        logger.notice("[SingleTabRootView] Creating RootView for Tab: \(tab.title) (\(tab.id.uuidString.prefix(8)))")

        let properties = makeProperties()
        let rootView = CanvasReactNativeHost.shared.createRootView(initialProperties: properties)

        if let rctRootView = rootView as? RCTRootView {
            context.coordinator.rootView = rctRootView
        }

        rootView.translatesAutoresizingMaskIntoConstraints = true
        rootView.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        rootView.backgroundColor = .clear

        if containerSize.width > 0 && containerSize.height > 0 {
            rootView.frame = CGRect(origin: .zero, size: containerSize)
            rootView.bounds = CGRect(origin: .zero, size: containerSize)
        }

        return rootView
        #else
        return UIView()
        #endif
    }

    func updateUIView(_ uiView: UIView, context: Context) {
        #if canImport(React)
        guard let rootView = uiView as? RCTRootView else { return }
        guard containerSize.width > 0 && containerSize.height > 0 else { return }

        let coordinator = context.coordinator
        let newFrame = CGRect(origin: .zero, size: containerSize)

        // 检查 isLeftPanelVisible 是否变化（立即更新，不等待 debounce）
        let currentIsLeftPanelVisible = viewModel.isLeftPanelVisible(for: tab.id)
        if coordinator.lastLeftPanelVisible != currentIsLeftPanelVisible {
            coordinator.lastLeftPanelVisible = currentIsLeftPanelVisible
            rootView.appProperties = self.makeProperties()
            logger.debug("[SingleTabRootView] isLeftPanelVisible 变化，立即更新 props: \(currentIsLeftPanelVisible)")
        }

        // 首次设置：确保 layer 和约束正确配置
        if coordinator.lastUpdatedSize == .zero {
            setupRootViewOnce(rootView, frame: newFrame)
        }

        // Native frame 始终立即更新（轻量操作）
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        rootView.frame = newFrame
        rootView.bounds = newFrame
        CATransaction.commit()

        // 拖动中：只更新 Yoga 布局（60fps 节流），不触发 React re-render
        coordinator.throttledYogaUpdate(newSize: containerSize) { [weak rootView] throttledSize in
            guard let rootView = rootView else { return }

            // 通知 Yoga 新尺寸（关键：解决缩小时不更新的问题）
            rootView.bridge.uiManager.setSize(throttledSize, for: rootView)

            // 强制布局更新
            rootView.setNeedsLayout()
            rootView.layoutIfNeeded()
        }

        // 拖动结束：延迟 150ms 后更新 React props（减少 re-render 次数）
        coordinator.scheduleDragEndUpdate { [weak rootView] in
            guard let rootView = rootView else { return }
            rootView.appProperties = self.makeProperties()
        }
        #endif
    }

    /// 首次设置 RootView 的 layer 和约束（只执行一次）
    private func setupRootViewOnce(_ rootView: RCTRootView, frame: CGRect) {
        // 设置 masksToBounds 防止 RN 内容溢出
        rootView.clipsToBounds = true

        rootView.frame = frame
        rootView.bounds = frame

        // 设置 contentView 的约束
        let contentView = rootView.contentView
        contentView.translatesAutoresizingMaskIntoConstraints = false
        contentView.clipsToBounds = true

        NSLayoutConstraint.activate([
            contentView.leadingAnchor.constraint(equalTo: rootView.leadingAnchor),
            contentView.trailingAnchor.constraint(equalTo: rootView.trailingAnchor),
            contentView.topAnchor.constraint(equalTo: rootView.topAnchor),
            contentView.bottomAnchor.constraint(equalTo: rootView.bottomAnchor)
        ])
    }

    private func makeProperties() -> [String: Any] {
        var tabDict: [String: Any] = [
            "id": tab.id.uuidString,
            "title": tab.title,
            "backgroundColor": tab.backgroundColor,
            "reloadKey": tab.reloadNonce
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

        // Single tab mode - 包装成数组以兼容旧版 JS 逻辑
        return [
            "singleTabMode": true,
            "tabs": [tabDict],
            "tab": tabDict,
            "currentTabId": tab.id.uuidString,
            "isLeftPanelVisible": viewModel.isLeftPanelVisible(for: tab.id),
            "isRightPanelVisible": tab.systemTabType == nil,
            // 显式传递容器尺寸，确保 RN 能感知窗口缩小（onLayout 可能不触发）
            "containerWidth": containerSize.width,
            "containerHeight": containerSize.height
        ]
    }

    class Coordinator: NSObject {
        let tab: Tab
        @ObservedObject var viewModel: BrowserViewModel
        #if canImport(React)
        var rootView: RCTRootView?
        #endif

        // 节流相关状态
        var lastUpdateTime: CFAbsoluteTime = 0
        var lastUpdatedSize: CGSize = .zero
        var lastLeftPanelVisible: Bool? = nil  // 跟踪左侧面板可见性变化
        var pendingSize: CGSize?
        var throttleWorkItem: DispatchWorkItem?
        var dragEndWorkItem: DispatchWorkItem?

        // 节流间隔：约 10ms (100fps)
        static let throttleInterval: CFAbsoluteTime = 0.010
        // 拖动结束判定延迟：20ms 无变化视为拖动结束
        static let dragEndDelay: CFAbsoluteTime = 0.020

        init(tab: Tab, viewModel: BrowserViewModel) {
            self.tab = tab
            self.viewModel = viewModel
        }

        deinit {
            throttleWorkItem?.cancel()
            dragEndWorkItem?.cancel()
            logger.debug("[SingleTabRootView] Coordinator deinit for Tab: \(self.tab.title)")
        }

        /// 拖动中更新：只更新 Yoga 布局，不触发 React re-render
        func throttledYogaUpdate(newSize: CGSize, yogaBlock: @escaping (CGSize) -> Void) {
            let now = CFAbsoluteTimeGetCurrent()
            let elapsed = now - lastUpdateTime

            // 如果尺寸没变，跳过
            if newSize == lastUpdatedSize {
                return
            }

            // 如果距离上次更新超过节流间隔，立即更新 Yoga
            if elapsed >= Self.throttleInterval {
                lastUpdateTime = now
                lastUpdatedSize = newSize
                throttleWorkItem?.cancel()
                pendingSize = nil
                yogaBlock(newSize)
            } else {
                // 否则，延迟到下一个节流周期
                pendingSize = newSize
                throttleWorkItem?.cancel()

                let workItem = DispatchWorkItem { [weak self] in
                    guard let self = self, let size = self.pendingSize else { return }
                    self.lastUpdateTime = CFAbsoluteTimeGetCurrent()
                    self.lastUpdatedSize = size
                    self.pendingSize = nil
                    yogaBlock(size)
                }
                throttleWorkItem = workItem
                let delay = Self.throttleInterval - elapsed
                DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: workItem)
            }
        }

        /// 拖动结束时更新：debounce 后触发 React props 更新
        func scheduleDragEndUpdate(propsBlock: @escaping () -> Void) {
            dragEndWorkItem?.cancel()
            let workItem = DispatchWorkItem {
                propsBlock()
            }
            dragEndWorkItem = workItem
            DispatchQueue.main.asyncAfter(deadline: .now() + Self.dragEndDelay, execute: workItem)
        }
    }
}
