import SwiftUI
import UIKit
import React
import React_RCTAppDelegate

private let logger = AppLogger.reactNative

/// Single Tab RootView - Every one. Tab Independent React Native RootView
/// One. Bridge + Multiple RootView Structure
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
        let rootView = ActriumReactNativeHost.shared.viewForTab(tabId: tab.id, initialProperties: properties)

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

        // Inspection isLeftPanelVisible Change (update immediately, no waiting) debounce)
        let currentIsLeftPanelVisible = viewModel.isLeftPanelVisible(for: tab.id)
        let sizeChanged = coordinator.lastUpdatedSize != containerSize
        let reloadKeyChanged = coordinator.lastReloadKey != tab.reloadNonce
        let leftPanelChanged = coordinator.lastLeftPanelVisible != currentIsLeftPanelVisible

        // 🛑 Guard: Skip if there is no change in key attributes — Prevention SwiftUI ↔ RN Feedback cycle
        if !leftPanelChanged && !reloadKeyChanged && !sizeChanged && coordinator.lastPropsSize == containerSize {
            return
        }

        if leftPanelChanged || reloadKeyChanged {
            coordinator.lastLeftPanelVisible = currentIsLeftPanelVisible
            coordinator.lastReloadKey = tab.reloadNonce
            rootView.appProperties = self.makeProperties()
            logger.debug("[SingleTabRootView] Key attribute change, update immediately props (tab: \(tab.title))")
        }

        // First set-up: ensure layer And discipline the right configuration
        if coordinator.lastUpdatedSize == .zero {
            setupRootViewOnce(rootView, frame: newFrame)
        }

        // Native frame Update only when size changes
        if sizeChanged {
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            rootView.frame = newFrame
            rootView.bounds = newFrame
            CATransaction.commit()
        }

        // Draging: Update only Yoga Layout (Performance)60fps No, no, no, no, no. React re-render
        coordinator.throttledYogaUpdate(newSize: containerSize) { [weak rootView] throttledSize in
            guard let rootView = rootView else { return }

            if let uiManager = (rootView.bridge as RCTBridge?)?.uiManager {
                uiManager.setSize(throttledSize, for: rootView)
            }

            rootView.setNeedsLayout()
            rootView.layoutIfNeeded()
        }

        // End of drag: Delay synchronisation after size changes React props
        if sizeChanged {
            coordinator.scheduleDragEndUpdate { [weak rootView, weak coordinator] in
                guard let rootView = rootView, let coordinator = coordinator else { return }

                if coordinator.lastPropsSize != containerSize {
                    coordinator.lastPropsSize = containerSize
                    rootView.appProperties = self.makeProperties()
                    logger.debug("[SingleTabRootView] Drag over. Sync the new dimensions to props: \(containerSize) (tab: \(tab.title))")
                }
            }
        }
        #endif
    }

    /// First setup RootView It's... layer and constraints (exercise only once)
    private func setupRootViewOnce(_ rootView: RCTRootView, frame: CGRect) {
        // Settings masksToBounds Prevention RN Spill
        rootView.clipsToBounds = true

        rootView.frame = frame
        rootView.bounds = frame

        // Settings contentView Constraints
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
        let runtimeProperties = tab.runtimeSingleRootProperties(
            isLeftPanelVisible: viewModel.isLeftPanelVisible(for: tab.id),
            containerHeight: runtimeDimension(containerSize.height),
            containerWidth: runtimeDimension(containerSize.width)
        )

        var tabDict = runtimeProperties.tab
        tabDict[RuntimeBridgeContract.status] = tab.status.rawValue

        // Add icon and appUrl if available
        if let icon = tab.icon {
            tabDict[RuntimeBridgeContract.icon] = icon
        }

        // System app tabs (History/Bookmarks)
        if let systemType = tab.systemTabType {
            tabDict[RuntimeBridgeContract.systemAppType] = systemType.rawValue

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
                tabDict[RuntimeBridgeContract.appUrl] = url.absoluteString
            }
            if let htmlContent = tab.htmlContent {
                tabDict[RuntimeTabPropsContract.htmlContent] = htmlContent
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

            // v2.0: Single Engine Structure
            if let unifiedScript = tab.unifiedScript {
                tabDict[RuntimeTabPropsContract.unifiedScript] = unifiedScript
                tabDict[RuntimeTabPropsContract.rightPanelDefaultVisible] = tab.rightPanelDefaultVisible
            }
            // Legacy: Double Engine Structure
            else if let leftScript = tab.leftPanelScript {
                tabDict["leftPanelScript"] = leftScript
                if let rightScript = tab.rightPanelScript {
                    tabDict["rightPanelScript"] = rightScript
                    tabDict[RuntimeTabPropsContract.rightPanelDefaultVisible] = tab.rightPanelDefaultVisible
                }
            }
            // Compatible old architecture
            else if let guestScript = tab.guestScript {
                tabDict["guestScript"] = guestScript
            }
        }

        var props = runtimeProperties.root
        props[RuntimeRootPropsContract.tab] = tabDict
        // Preserve the previous UIKit-provided numeric values for iOS layout.
        props[RuntimeBridgeContract.containerWidth] = containerSize.width
        props[RuntimeBridgeContract.containerHeight] = containerSize.height
        props["appId"] = tab.appId ?? ""

        // Merge extra parameters (from openTab (Aggregations to be imported)
        if let additionalProps = tab.additionalProps {
            for (key, value) in additionalProps {
                props[key] = value
            }
        }

        return props
    }

    private func runtimeDimension(_ value: CGFloat) -> Int32 {
        Int32(clamping: Int(value.rounded()))
    }

    @MainActor
    class Coordinator: NSObject {
        let tab: Tab
        var viewModel: BrowserViewModel
        #if canImport(React)
        var rootView: RCTRootView?
        #endif

        // Scalable Associated Status
        var lastUpdateTime: CFAbsoluteTime = 0
        var lastUpdatedSize: CGSize = .zero
        var lastLeftPanelVisible: Bool? = nil  // Tracks the visible changes on the left panel
        var lastReloadKey: Int = 0             // Track Load State
        var lastPropsSize: CGSize = .zero      // Track Last Sync props Size
        var pendingSize: CGSize?
        var throttleWorkItem: DispatchWorkItem?
        var dragEndWorkItem: DispatchWorkItem?

        // Spacing: about 10ms (100fps)
        static let throttleInterval: CFAbsoluteTime = 0.010
        // Drag the end of the decision delay:20ms No change is considered to be the end of drag
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

        /// Drag to update: update only Yoga Layout, no trigger. React re-render
        func throttledYogaUpdate(newSize: CGSize, yogaBlock: @escaping (CGSize) -> Void) {
            let now = CFAbsoluteTimeGetCurrent()
            let elapsed = now - lastUpdateTime

            // If the size doesn't change, skip.
            if newSize == lastUpdatedSize {
                return
            }

            // Update immediately if the last update exceeds the current interval Yoga
            if elapsed >= Self.throttleInterval {
                lastUpdateTime = now
                lastUpdatedSize = newSize
                throttleWorkItem?.cancel()
                pendingSize = nil
                yogaBlock(newSize)
            } else {
                // Otherwise, delay to the next suspense cycle
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

        /// Update at drag:debounce After Trigger React props Update
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
