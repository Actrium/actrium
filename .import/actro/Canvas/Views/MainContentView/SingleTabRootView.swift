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
    let containerSize: CGSize

    var body: some View {
        SingleTabRootRepresentable(
            tab: tab,
            viewModel: viewModel,
            historyVM: historyVM,
            containerSize: containerSize
        )
        .id(tab.id)
    }
}

struct SingleTabRootRepresentable: UIViewRepresentable {
    let tab: Tab
    @ObservedObject var viewModel: BrowserViewModel
    var historyVM: HistoryViewModel?
    let containerSize: CGSize

    func makeCoordinator() -> Coordinator {
        Coordinator(tab: tab, viewModel: viewModel)
    }

    func makeUIView(context: Context) -> UIView {
        #if canImport(React)
        logger.notice("[SingleTabRootView] Getting RootView for Tab: \(tab.title) (\(tab.id.uuidString.prefix(8)))")

        let properties = makeProperties()
        let rootView = CanvasReactNativeHost.shared.viewForTab(tabId: tab.id, initialProperties: properties)

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
        let sizeChanged = coordinator.lastUpdatedSize != containerSize
        let reloadKeyChanged = coordinator.lastReloadKey != tab.reloadNonce
        let leftPanelChanged = coordinator.lastLeftPanelVisible != currentIsLeftPanelVisible

        // 🛑 守卫：如果没有任何关键属性变化，直接跳过 — 防止 SwiftUI ↔ RN 反馈循环
        if !leftPanelChanged && !reloadKeyChanged && !sizeChanged && coordinator.lastPropsSize == containerSize {
            return
        }

        if leftPanelChanged || reloadKeyChanged {
            coordinator.lastLeftPanelVisible = currentIsLeftPanelVisible
            coordinator.lastReloadKey = tab.reloadNonce
            rootView.appProperties = self.makeProperties()
            logger.debug("[SingleTabRootView] 关键属性变化，立即更新 props (tab: \(tab.title))")
        }

        // 首次设置：确保 layer 和约束正确配置
        if coordinator.lastUpdatedSize == .zero {
            setupRootViewOnce(rootView, frame: newFrame)
        }

        // Native frame 仅在尺寸变化时更新
        if sizeChanged {
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            rootView.frame = newFrame
            rootView.bounds = newFrame
            CATransaction.commit()
        }

        // 拖动中：只更新 Yoga 布局（60fps 节流），不触发 React re-render
        coordinator.throttledYogaUpdate(newSize: containerSize) { [weak rootView] throttledSize in
            guard let rootView = rootView else { return }

            if let uiManager = (rootView.bridge as RCTBridge?)?.uiManager {
                uiManager.setSize(throttledSize, for: rootView)
            }

            rootView.setNeedsLayout()
            rootView.layoutIfNeeded()
        }

        // 拖动结束：尺寸变化后延迟同步到 React props
        if sizeChanged {
            coordinator.scheduleDragEndUpdate { [weak rootView, weak coordinator] in
                guard let rootView = rootView, let coordinator = coordinator else { return }

                if coordinator.lastPropsSize != containerSize {
                    coordinator.lastPropsSize = containerSize
                    rootView.appProperties = self.makeProperties()
                    logger.debug("[SingleTabRootView] 拖动结束，同步新尺寸到 props: \(containerSize) (tab: \(tab.title))")
                }
            }
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
            "reloadKey": tab.reloadNonce,
            "status": tab.status.rawValue
        ]

        // Add icon and appUrl if available
        if let icon = tab.icon {
            tabDict["icon"] = icon
        }

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
                break
            case .login:
                // Login tab currently doesn't need extra data from native
                break
            case .settings:
                // Settings tab currently doesn't need extra data from native
                break
            case .globalSearch:
                // Global search tab currently doesn't need extra data from native
                break
            }
        } else {
            // Regular ACTR tabs
            if let url = tab.url {
                tabDict["appUrl"] = url.absoluteString
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

        // Single tab mode — 只传 tab 单对象，不再冗余包装成 tabs 数组
        var props: [String: Any] = [
            "singleTabMode": true,
            "tab": tabDict,
            "currentTabId": tab.id.uuidString,
            "isLeftPanelVisible": viewModel.isLeftPanelVisible(for: tab.id),
            "isRightPanelVisible": tab.systemTabType == nil,
            // 显式传递容器尺寸，确保 RN 能感知窗口缩小（onLayout 可能不触发）
            "containerWidth": containerSize.width,
            "containerHeight": containerSize.height,
            "appUrl": tab.url?.absoluteString ?? "",
            "appId": tab.appId ?? ""
        ]
        
        // 合并额外参数（从 openTab 传入的参数）
        if let additionalProps = tab.additionalProps {
            for (key, value) in additionalProps {
                props[key] = value
            }
        }
        
        return props
    }

    @MainActor
    class Coordinator: NSObject {
        let tab: Tab
        var viewModel: BrowserViewModel
        #if canImport(React)
        var rootView: RCTRootView?
        #endif

        // 节流相关状态
        var lastUpdateTime: CFAbsoluteTime = 0
        var lastUpdatedSize: CGSize = .zero
        var lastLeftPanelVisible: Bool? = nil  // 跟踪左侧面板可见性变化
        var lastReloadKey: Int = 0             // 跟踪重载状态
        var lastPropsSize: CGSize = .zero      // 跟踪最后一次同步到 props 的尺寸
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
