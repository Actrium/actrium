import SwiftUI
import AppKit

#if canImport(React)
import React
#endif

struct ReactNativeTabHostView: View {
    @ObservedObject var viewModel: BrowserViewModel
    let isLeftPanelVisible: Bool
    let isRightPanelVisible: Bool
    var onCoordinatorReady: ((ReactNativeRootRepresentable.Coordinator) -> Void)? = nil

    var body: some View {
        GeometryReader { geometry in
            ReactNativeRootRepresentable(
                viewModel: viewModel,
                tabs: viewModel.tabs,
                currentTabId: viewModel.currentTabId,
                isLeftPanelVisible: isLeftPanelVisible,
                isRightPanelVisible: isRightPanelVisible,
                containerSize: geometry.size,
                onCoordinatorReady: onCoordinatorReady
            )
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

struct ReactNativeRootRepresentable: NSViewRepresentable {
    @ObservedObject var viewModel: BrowserViewModel
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
        let containerView = NSView()

        let properties = makeProperties()
        let rootView = CanvasReactNativeHost.shared
            .createRootView(initialProperties: properties)

        if let rctRootView = rootView as? RCTRootView {
            context.coordinator.rootView = rctRootView
        }

        rootView.translatesAutoresizingMaskIntoConstraints = false
        containerView.addSubview(rootView)

        NSLayoutConstraint.activate([
            rootView.leadingAnchor.constraint(equalTo: containerView.leadingAnchor),
            rootView.trailingAnchor.constraint(equalTo: containerView.trailingAnchor),
            rootView.topAnchor.constraint(equalTo: containerView.topAnchor),
            rootView.bottomAnchor.constraint(equalTo: containerView.bottomAnchor)
        ])

        return containerView
        #else
        return NSView()
        #endif
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        #if canImport(React)
        let properties = makeProperties()

        let previousTabId = context.coordinator.previousTabId
        let newTabId = currentTabId

        if let prevId = previousTabId, prevId != newTabId, let _ = context.coordinator.rootView {
            context.coordinator.captureSnapshot(for: prevId)
        }

        context.coordinator.previousTabId = newTabId

        if let rootView = context.coordinator.rootView {
            rootView.appProperties = properties
            rootView.setFrameSize(nsView.bounds.size)
            rootView.needsLayout = true
            rootView.layoutSubtreeIfNeeded()
        } else if let rootView = nsView as? RCTRootView {
            rootView.appProperties = properties
            rootView.setFrameSize(nsView.bounds.size)
            rootView.needsLayout = true
            rootView.layoutSubtreeIfNeeded()
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
            if let url = tab.url {
                dict["url"] = url.absoluteString
            }
            if let htmlContent = tab.htmlContent {
                dict["htmlContent"] = htmlContent
            }

            // 优先使用新架构 (leftPanel + rightPanel)
            if let leftScript = tab.leftPanelScript {
                dict["leftPanelScript"] = leftScript
                print("✅ 传递 leftPanelScript 到 RN，长度: \(leftScript.count)")
            }

            if let rightScript = tab.rightPanelScript {
                dict["rightPanelScript"] = rightScript
                dict["rightPanelDefaultVisible"] = tab.rightPanelDefaultVisible
                print("✅ 传递 rightPanelScript 到 RN，长度: \(rightScript.count), 默认\(tab.rightPanelDefaultVisible ? "显示" : "隐藏")")
            }

            // 向后兼容旧架构 (单一 guestScript)
            if let guestScript = tab.guestScript {
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

        func captureSnapshot(for tabId: UUID) {
            #if canImport(React)
            guard let rootView = rootView else { return }

            Task { @MainActor in
                let snapshot = await rootView.captureSnapshot()

                if let snapshot = snapshot {
                    let scaled = snapshot.scaledToFit(maxDimension: SnapshotQuality.medium.maxDimension)
                    viewModel.updateTabSnapshot(image: scaled, forTabId: tabId)
                }
            }
            #endif
        }

        func captureCurrentSnapshot() {
            guard let currentTabId = viewModel.currentTabId else { return }
            captureSnapshot(for: currentTabId)
        }
    }
}
