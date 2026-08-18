import SwiftUI
import Highlightr
import AppKit
import UniformTypeIdentifiers

/// Main DevTools panel view - used both for docked mode and detached window
struct DevToolsPanel: View {
    @ObservedObject var devTools: DevToolsManager
    @ObservedObject var viewModel: BrowserViewModel

    /// Whether this panel is in detached window mode
    var isDetached: Bool = false

    /// Whether the DevTools is active (false when in Overview mode)
    var isActive: Bool = true

    /// Determine what type of DevTools content to show
    private enum DevToolsMode {
        case actr           // Actr app - show our custom DevTools
        case web            // Web tab - use WebKit Inspector
        case unavailable    // System tabs, empty - not available
    }

    private var devToolsMode: DevToolsMode {
        guard let tab = viewModel.currentTab else { return .unavailable }

        switch tab.contentType {
        case .web:
            return .web
        case .reactNative:
            // Actr apps have no systemTabType, system tabs (history/bookmarks) have one
            return tab.systemTabType == nil ? .actr : .unavailable
        case .native, .empty:
            // Native SwiftUI and empty content have no DevTools
            return .unavailable
        }
    }

    /// Check if current DevTools tab is a global tool (always available)
    private var isGlobalTab: Bool {
        devTools.activeTab == .metro || devTools.activeTab == .monitor
    }

    var body: some View {
        VStack(spacing: 0) {
            // Toolbar at top (WebKit Inspector style)
            DevToolsToolbar(
                devTools: devTools,
                viewModel: viewModel,
                isDetached: isDetached
            )

            Divider()
                .background(Color(white: 0.25))

            // Content area below toolbar
            if !isActive {
                // Overview mode - but global tabs (Metro, Monitor) are still available
                if isGlobalTab {
                    globalTabContent
                } else {
                    DevToolsInactivePlaceholder()
                }
            } else {
                switch devToolsMode {
                case .actr:
                    // Actr/React Native app - show full DevTools
                    DevToolsContent(devTools: devTools, viewModel: viewModel)
                case .web:
                    // Web tab - global tabs available, others use WebKit Inspector
                    if isGlobalTab {
                        globalTabContent
                    } else {
                        DevToolsWebPlaceholder()
                    }
                case .unavailable:
                    // System tabs or empty - only global tabs available
                    if isGlobalTab {
                        globalTabContent
                    } else {
                        DevToolsUnavailablePlaceholder()
                    }
                }
            }
        }
        .background(Color(red: 0.11, green: 0.11, blue: 0.11))
    }

    /// Content view for global tabs (Metro, Monitor)
    @ViewBuilder
    private var globalTabContent: some View {
        switch devTools.activeTab {
        case .metro:
            MetroPanel()
        case .monitor:
            DevToolsMonitorView(devTools: devTools, viewModel: viewModel)
        default:
            EmptyView()
        }
    }
}

/// Placeholder view shown when DevTools is not active (e.g., in Overview mode)
private struct DevToolsInactivePlaceholder: View {
    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: "square.grid.2x2")
                .font(.system(size: 32))
                .foregroundColor(Color(white: 0.4))
            Text("DevTools unavailable in Overview mode")
                .font(.system(size: 13))
                .foregroundColor(Color(white: 0.5))
            Text("Switch to a tab to use DevTools")
                .font(.system(size: 11))
                .foregroundColor(Color(white: 0.4))
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(red: 0.11, green: 0.11, blue: 0.11))
    }
}

/// Placeholder view shown for Web tabs (non-Actr apps)
private struct DevToolsWebPlaceholder: View {
    var body: some View {
        VStack(spacing: 16) {
            Image(systemName: "globe")
                .font(.system(size: 36))
                .foregroundColor(Color(white: 0.4))

            VStack(spacing: 6) {
                Text("DevTools unavailable for Web content")
                    .font(.system(size: 13, weight: .medium))
                    .foregroundColor(Color(white: 0.5))

                Text("Use WebKit Inspector for web page debugging")
                    .font(.system(size: 12))
                    .foregroundColor(Color(white: 0.45))
            }

            // Keyboard shortcut hint
            HStack(spacing: 4) {
                Image(systemName: "command")
                    .font(.system(size: 11))
                Image(systemName: "option")
                    .font(.system(size: 11))
                Text("I")
                    .font(.system(size: 11, weight: .semibold))
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .background(Color(white: 0.2))
            .cornerRadius(6)
            .foregroundColor(Color(white: 0.6))
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(red: 0.11, green: 0.11, blue: 0.11))
    }
}

/// Placeholder view shown for system tabs (history, bookmarks, etc.)
private struct DevToolsUnavailablePlaceholder: View {
    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: "wrench.and.screwdriver")
                .font(.system(size: 32))
                .foregroundColor(Color(white: 0.4))
            Text("DevTools not available")
                .font(.system(size: 13))
                .foregroundColor(Color(white: 0.5))
            Text("Switch to an Actr app to use DevTools")
                .font(.system(size: 11))
                .foregroundColor(Color(white: 0.4))
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(red: 0.11, green: 0.11, blue: 0.11))
    }
}

/// DevTools toolbar - WebKit Inspector pixel-perfect clone
struct DevToolsToolbar: View {
    @ObservedObject var devTools: DevToolsManager
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject private var metro = MetroManager.shared
    var isDetached: Bool
    @Environment(\.openWindow) private var openWindow
    @State private var draggingTab: DevToolsTab?

    // WebKit Inspector exact colors
    private let toolbarBg = Color(red: 0.145, green: 0.145, blue: 0.145)  // #252525
    private let selectedTabBg = Color(red: 0.22, green: 0.22, blue: 0.22)  // Selected tab fill
    private let iconColor = Color(white: 0.55)
    private let iconActiveColor = Color(white: 0.90)
    private let textColor = Color(white: 0.65)
    private let textSelectedColor = Color.white
    private let separatorColor = Color(white: 0.30)

    var body: some View {
        HStack(spacing: 0) {
            // === Dock icons (WebKit: no X button shown, dock icons first) ===
            HStack(spacing: 0) {
                // Icon 1: Dock right (two vertical split)
                Button(action: {
                    if !isDetached {
                        devTools.dockPosition = .right
                        devTools.saveSettings()
                    }
                }) {
                    Image(systemName: "rectangle.lefthalf.inset.filled.arrow.left")
                        .font(.system(size: 15))
                        .foregroundColor(!isDetached && devTools.dockPosition == .right ? iconActiveColor : iconColor)
                }
                .buttonStyle(.plain)
                .frame(width: 28, height: 36)

                // Icon 2: Dock bottom (two horizontal split)
                Button(action: {
                    if !isDetached {
                        devTools.dockPosition = .bottom
                        devTools.saveSettings()
                    }
                }) {
                    Image(systemName: "rectangle.bottomhalf.inset.filled.arrow.down")
                        .font(.system(size: 15))
                        .foregroundColor(!isDetached && devTools.dockPosition == .bottom ? iconActiveColor : iconColor)
                }
                .buttonStyle(.plain)
                .frame(width: 28, height: 36)

                // Icon 3: Detach (pip-style window)
                Button(action: {
                    if isDetached { devTools.dock() }
                    else {
                        devTools.detach()
                        openWindow(id: "devtools")
                    }
                }) {
                    Image(systemName: "pip.exit")
                        .font(.system(size: 15))
                        .foregroundColor(isDetached ? iconActiveColor : iconColor)
                }
                .buttonStyle(.plain)
                .frame(width: 28, height: 36)

                // Icon 4: Element selector (crosshair)
                Button(action: {}) {
                    Image(systemName: "scope")
                        .font(.system(size: 15))
                        .foregroundColor(iconColor)
                }
                .buttonStyle(.plain)
                .frame(width: 28, height: 36)

                // Icon 5: Responsive/device
                Button(action: {}) {
                    Image(systemName: "display")
                        .font(.system(size: 14))
                        .foregroundColor(iconColor)
                }
                .buttonStyle(.plain)
                .frame(width: 28, height: 36)
            }
            .padding(.leading, 4)

            // === TABS: WebKit-style - NO border, just icon+text, selected has fill ===
            HStack(spacing: 0) {
                ForEach(devTools.tabOrder) { tab in
                    let isSelected = devTools.activeTab == tab
                    let statusColor: Color? = (tab == .metro && metro.state.isRunning) ? .green : nil

                    Button(action: { devTools.activeTab = tab }) {
                        HStack(spacing: 6) {
                            Image(systemName: tab.icon)
                                .font(.system(size: 12))
                            Text(tab.rawValue)
                                .font(.system(size: 12, weight: .regular))
                        }
                        .foregroundColor(statusColor ?? (isSelected ? textSelectedColor : textColor))
                        .padding(.horizontal, 14)
                        .padding(.vertical, 6)
                        .background(
                            // WebKit: selected tab = dark fill, no border
                            RoundedRectangle(cornerRadius: 5)
                                .fill(isSelected ? selectedTabBg : Color.clear)
                        )
                    }
                    .buttonStyle(.plain)
                    .opacity(draggingTab == tab ? 0.5 : 1.0)
                    .onDrag {
                        draggingTab = tab
                        return NSItemProvider(object: tab.rawValue as NSString)
                    }
                    .onDrop(of: [.text], delegate: DevToolsTabDropDelegate(
                        tab: tab, devTools: devTools, draggingTab: $draggingTab
                    ))
                }
            }
            .padding(.leading, 4)

            Spacer()

            // === RIGHT: Pause indicator (for RN apps) ===
            if let currentTab = viewModel.currentTab,
               case .reactNative = currentTab.contentType,
               currentTab.systemTabType == nil {
                let isConnected = devTools.engineMonitorSnapshot?.rows.contains { $0.tabId == currentTab.id.uuidString } ?? false
                let indicatorColor: Color = !isConnected ? .gray : (devTools.isEnginePaused ? .yellow : .green)

                Button(action: { devTools.togglePause() }) {
                    Circle().fill(indicatorColor).frame(width: 8, height: 8)
                }
                .buttonStyle(.plain)
                .disabled(!isConnected)
                .padding(.trailing, 8)
            }

            // === Search button ===
            Rectangle().fill(separatorColor).frame(width: 1, height: 20)
            Button(action: {}) {
                Image(systemName: "magnifyingglass")
                    .font(.system(size: 13))
                    .foregroundColor(iconColor)
            }
            .buttonStyle(.plain)
            .frame(width: 32, height: 36)

            // === Settings button ===
            Rectangle().fill(separatorColor).frame(width: 1, height: 20)
            Button(action: {}) {
                Image(systemName: "gearshape")
                    .font(.system(size: 14))
                    .foregroundColor(iconColor)
            }
            .buttonStyle(.plain)
            .frame(width: 32, height: 36)
        }
        .frame(height: 36)
        .background(toolbarBg)
        .contextMenu { }
    }
}

/// Individual tab button in the toolbar - WebKit Inspector cell style
struct DevToolsTabButton: View {
    let tab: DevToolsTab
    let isSelected: Bool
    var isDragging: Bool = false
    var statusColor: Color? = nil
    let action: () -> Void

    var body: some View {
        // WebKit style: selected tab has subtle background highlight
        let textColor: Color = {
            if let status = statusColor { return status }
            return isSelected ? Color.white : Color(white: 0.6)
        }()

        Button(action: action) {
            HStack(spacing: 5) {
                Image(systemName: tab.icon)
                    .font(.system(size: 12))
                Text(tab.rawValue)
                    .font(.system(size: 12, weight: .regular))
            }
            .foregroundColor(textColor)
            .frame(minWidth: 80, maxWidth: 120, minHeight: 28)
            .background(isSelected ? Color(white: 0.22) : Color.clear)
            .opacity(isDragging ? 0.5 : 1.0)
        }
        .buttonStyle(.plain)
    }
}

/// Drop delegate for handling tab reordering
struct DevToolsTabDropDelegate: DropDelegate {
    let tab: DevToolsTab
    let devTools: DevToolsManager
    @Binding var draggingTab: DevToolsTab?

    func performDrop(info: DropInfo) -> Bool {
        draggingTab = nil
        devTools.saveSettings()
        return true
    }

    func dropEntered(info: DropInfo) {
        guard let dragging = draggingTab,
              dragging != tab,
              let fromIndex = devTools.tabOrder.firstIndex(of: dragging),
              let toIndex = devTools.tabOrder.firstIndex(of: tab) else { return }

        withAnimation(.easeInOut(duration: 0.2)) {
            devTools.tabOrder.move(
                fromOffsets: IndexSet(integer: fromIndex),
                toOffset: toIndex > fromIndex ? toIndex + 1 : toIndex
            )
        }
    }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        DropProposal(operation: .move)
    }

    func dropExited(info: DropInfo) {
        // No action needed
    }

    func validateDrop(info: DropInfo) -> Bool {
        draggingTab != nil
    }
}

/// Content area that switches based on active tab
struct DevToolsContent: View {
    @ObservedObject var devTools: DevToolsManager
    @ObservedObject var viewModel: BrowserViewModel

    var body: some View {
        Group {
            switch devTools.activeTab {
            case .messages:
                DevToolsMessagesView(devTools: devTools)
            case .elements:
                DevToolsElementsView(devTools: devTools, viewModel: viewModel)
            case .sources:
                SourceViewerPanel()
            case .console:
                DevToolsConsoleView(devTools: devTools)
            case .network:
                NetworkSequenceView(devTools: devTools, viewModel: viewModel)
            case .monitor:
                DevToolsMonitorView(devTools: devTools, viewModel: viewModel)
            case .metro:
                MetroPanel()
            }
        }
        .onAppear {
            updateGuestAppSource()
        }
        .onChange(of: viewModel.currentTabId) { _, _ in
            updateGuestAppSource()
        }
    }

    private func updateGuestAppSource() {
        guard let tab = viewModel.currentTab else {
            devTools.updateGuestAppSource(path: nil, name: nil)
            return
        }

        // Get the source directory from tab's URL
        if let url = tab.url {
            // For file URLs, get the directory path
            let path = url.isFileURL ? url.deletingLastPathComponent().path : url.path
            devTools.updateGuestAppSource(path: path, name: tab.title)
        } else {
            devTools.updateGuestAppSource(path: nil, name: tab.title)
        }
    }
}

/// Placeholder view for unimplemented tabs
struct DevToolsPlaceholderView: View {
    let title: String
    let message: String

    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: "hammer.fill")
                .font(.system(size: 32))
                .foregroundColor(.secondary)
            Text(title)
                .font(.headline)
                .foregroundColor(.primary)
            Text(message)
                .font(.subheadline)
                .foregroundColor(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(nsColor: .textBackgroundColor))
    }
}

// MARK: - Elements Tab

struct DevToolsElementsView: View {
    @ObservedObject var devTools: DevToolsManager
    @ObservedObject var viewModel: BrowserViewModel
    @FocusState private var isTreeFocused: Bool

    /// Build flat list of visible nodes for keyboard navigation
    private var visibleNodes: [DevToolsComponentNode] {
        guard let tree = devTools.componentTree else { return [] }
        var result: [DevToolsComponentNode] = []
        collectVisibleNodes(tree, into: &result)
        return result
    }

    private func collectVisibleNodes(_ node: DevToolsComponentNode, into result: inout [DevToolsComponentNode]) {
        // Skip __TEXT__ nodes - they are displayed inline in parent
        guard node.type != "__TEXT__" else { return }

        result.append(node)

        // Check if all children are __TEXT__ (inline text mode)
        let hasOnlyTextChildren = !node.children.isEmpty &&
            node.children.allSatisfy { $0.type == "__TEXT__" }

        // Don't traverse into children if they're all __TEXT__ (shown inline)
        if !hasOnlyTextChildren {
            if devTools.isNodeExpanded(node.id) || (devTools.expandedNodeIds.isEmpty && !node.children.isEmpty) {
                // Default: expand all if no explicit state set
                for child in node.children {
                    collectVisibleNodes(child, into: &result)
                }
            }
        }
    }

    var body: some View {
        HSplitView {
            // Component tree (left)
            VStack(alignment: .leading, spacing: 0) {
                HStack(spacing: 8) {
                    Text("Component Tree")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundColor(.secondary)

                    Spacer()

                    // Inspect mode button (our custom implementation)
                    Button(action: { devTools.toggleInspect() }) {
                        HStack(spacing: 4) {
                            Image(systemName: "cursorarrow.click.2")
                                .font(.system(size: 10))
                            if devTools.isInspectMode {
                                Text("Inspecting")
                                    .font(.system(size: 10))
                            }
                        }
                        .foregroundColor(devTools.isInspectMode ? .accentColor : .secondary)
                    }
                    .buttonStyle(.borderless)
                    .help(devTools.isInspectMode ? "Exit Inspect Mode" : "Inspect Element")

                    // Node count
                    if let tree = devTools.componentTree {
                        Text("\(countNodes(tree))")
                            .font(.system(size: 10))
                            .foregroundColor(Color(nsColor: .tertiaryLabelColor))
                    }
                }
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .background(Color(nsColor: .controlBackgroundColor))
                .contextMenu { }

                Divider()

                if let tree = devTools.componentTree {
                    ScrollViewReader { proxy in
                        ScrollView {
                            LazyVStack(alignment: .leading, spacing: 0) {
                                DevToolsTreeNodeView(node: tree, depth: 0, devTools: devTools)
                            }
                            .padding(4)
                        }
                        .focusable()
                        .focused($isTreeFocused)
                        .focusEffectDisabled()  // Remove blue border on focus
                        .onKeyPress(.upArrow) {
                            navigateTree(direction: -1)
                            return .handled
                        }
                        .onKeyPress(.downArrow) {
                            navigateTree(direction: 1)
                            return .handled
                        }
                        .onKeyPress(.leftArrow) {
                            collapseOrMoveUp()
                            return .handled
                        }
                        .onKeyPress(.rightArrow) {
                            expandOrMoveDown()
                            return .handled
                        }
                        .onAppear {
                            // Auto-expand all nodes on first load
                            if devTools.expandedNodeIds.isEmpty {
                                devTools.expandAllNodes()
                            }
                        }
                        // Ensure selected item stays visible after any change
                        .onChange(of: devTools.selectedNodeId) { _, newId in
                            if let nodeId = newId {
                                // Small delay to let layout settle after expand/collapse
                                DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) {
                                    proxy.scrollTo(nodeId)
                                }
                            }
                        }
                        // Also handle expand/collapse state changes
                        .onChange(of: devTools.expandedNodeIds) { _, _ in
                            if let nodeId = devTools.selectedNodeId {
                                DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) {
                                    proxy.scrollTo(nodeId)
                                }
                            }
                        }
                    }
                } else {
                    VStack(spacing: 8) {
                        Image(systemName: "square.stack.3d.up")
                            .font(.system(size: 24))
                            .foregroundColor(.secondary)
                        Text("No component tree available")
                            .font(.system(size: 12))
                            .foregroundColor(.secondary)
                        Text("Components will appear when the app renders")
                            .font(.system(size: 11))
                            .foregroundColor(Color(nsColor: .tertiaryLabelColor))
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
            .frame(minWidth: 200)

            // Props inspector (right)
            VStack(alignment: .leading, spacing: 0) {
                HStack {
                    Text("Properties")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundColor(.secondary)
                    Spacer()
                }
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .background(Color(nsColor: .controlBackgroundColor))

                Divider()

                if let nodeId = devTools.selectedNodeId,
                   let node = findNode(in: devTools.componentTree, id: nodeId) {
                    ScrollView {
                        VStack(alignment: .leading, spacing: 12) {
                            // Node info header
                            HStack(spacing: 6) {
                                Text("<\(node.type)>")
                                    .font(.system(size: 13, weight: .semibold, design: .monospaced))
                                    .foregroundColor(.accentColor)
                                    .textSelection(.enabled)
                                Text("#\(node.id)")
                                    .font(.system(size: 11, design: .monospaced))
                                    .foregroundColor(.secondary)
                                    .textSelection(.enabled)
                            }

                            Divider()

                            // Box Model visualization (if available)
                            if let boxModel = devTools.boxModel {
                                DevToolsBoxModelView(boxModel: boxModel)
                            }

                            // Computed Styles (if available)
                            if let styles = devTools.computedStyles {
                                DevToolsComputedStylesView(styles: styles)
                            }

                            // Props section
                            VStack(alignment: .leading, spacing: 6) {
                                Text("Props")
                                    .font(.system(size: 10, weight: .semibold))
                                    .foregroundColor(.secondary)

                                if node.props.isEmpty {
                                    Text("No props")
                                        .font(.system(size: 11))
                                        .foregroundColor(.secondary)
                                        .italic()
                                } else {
                                    ForEach(Array(node.props.keys.sorted()), id: \.self) { key in
                                        DevToolsPropRowView(key: key, value: node.props[key])
                                    }
                                }
                            }
                            .padding(8)
                            .background(Color(nsColor: .controlBackgroundColor).opacity(0.5))
                            .cornerRadius(6)

                            // Children count
                            if !node.children.isEmpty {
                                HStack {
                                    Text("Children:")
                                        .font(.system(size: 11))
                                        .foregroundColor(.secondary)
                                    Text("\(node.children.count)")
                                        .font(.system(size: 11, design: .monospaced))
                                        .foregroundColor(.primary)
                                }
                            }
                        }
                        .padding(8)
                    }
                } else {
                    VStack(spacing: 8) {
                        Image(systemName: "sidebar.right")
                            .font(.system(size: 20))
                            .foregroundColor(.secondary)
                        Text("Select a component")
                            .font(.system(size: 12))
                            .foregroundColor(.secondary)
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
            .frame(minWidth: 200)
        }
        .background(Color(nsColor: .textBackgroundColor))
    }

    // MARK: - Keyboard Navigation

    /// Navigate up or down in the tree
    private func navigateTree(direction: Int) {
        let nodes = visibleNodes
        guard !nodes.isEmpty else { return }

        if let currentId = devTools.selectedNodeId,
           let currentIndex = nodes.firstIndex(where: { $0.id == currentId }) {
            let newIndex = max(0, min(nodes.count - 1, currentIndex + direction))
            devTools.selectNode(nodes[newIndex].id)
        } else {
            // No selection, select first or last
            devTools.selectNode(direction > 0 ? nodes.first?.id : nodes.last?.id)
        }
        // Scrolling is handled by .onChange(of: selectedNodeId)
    }

    /// Left arrow: collapse current node, or move to parent
    private func collapseOrMoveUp() {
        guard let currentId = devTools.selectedNodeId,
              let currentNode = findNode(in: devTools.componentTree, id: currentId) else { return }

        // If node is expanded and has children, collapse it
        if !currentNode.children.isEmpty && devTools.isNodeExpanded(currentId) {
            devTools.collapseNode(currentId)
        } else {
            // Move to parent
            if let parentId = findParentId(of: currentId, in: devTools.componentTree) {
                devTools.selectNode(parentId)
            }
        }
    }

    /// Right arrow: expand current node, or move to first child
    private func expandOrMoveDown() {
        guard let currentId = devTools.selectedNodeId,
              let currentNode = findNode(in: devTools.componentTree, id: currentId) else { return }

        guard !currentNode.children.isEmpty else { return }

        // If node is collapsed, expand it
        if !devTools.isNodeExpanded(currentId) {
            devTools.expandNode(currentId)
        } else {
            // Move to first child
            if let firstChild = currentNode.children.first {
                devTools.selectNode(firstChild.id)
            }
        }
    }

    /// Find parent node ID
    private func findParentId(of nodeId: String, in tree: DevToolsComponentNode?, parentId: String? = nil) -> String? {
        guard let tree = tree else { return nil }
        if tree.id == nodeId { return parentId }
        for child in tree.children {
            if let found = findParentId(of: nodeId, in: child, parentId: tree.id) {
                return found
            }
        }
        return nil
    }

    /// Count total nodes in tree (excluding __TEXT__ nodes)
    private func countNodes(_ node: DevToolsComponentNode) -> Int {
        // Skip __TEXT__ nodes in count
        guard node.type != "__TEXT__" else { return 0 }
        return 1 + node.children.reduce(0) { $0 + countNodes($1) }
    }

    /// Find a node by ID in the tree
    private func findNode(in tree: DevToolsComponentNode?, id: String) -> DevToolsComponentNode? {
        guard let tree = tree else { return nil }
        if tree.id == id { return tree }
        for child in tree.children {
            if let found = findNode(in: child, id: id) {
                return found
            }
        }
        return nil
    }
}

/// Single prop row view
struct DevToolsPropRowView: View {
    let key: String
    let value: Any?
    var depth: Int = 0

    @State private var isExpanded: Bool = false

    /// Check if value is a serialized function with source location
    private var functionInfo: (name: String, source: String?, sourceFile: String?, sourceLine: Int?)? {
        guard let dict = value as? [String: Any],
              let type = dict["__type"] as? String,
              type == "function" else { return nil }
        let name = dict["__name"] as? String ?? dict["name"] as? String ?? "anonymous"
        let source = dict["source"] as? String
        // Source location is injected by Babel plugin at compile time
        let sourceFile = dict["__sourceFile"] as? String
        let sourceLine = dict["__sourceLine"] as? Int
        return (name, source, sourceFile, sourceLine)
    }

    /// Check if value is expandable (object or array with items)
    /// Note: functions are now clickable links, not expandable
    private var isExpandable: Bool {
        // Functions are clickable links, not expandable
        if functionInfo != nil {
            return false
        }
        switch value {
        case let dict as [String: Any]:
            return !dict.isEmpty
        case let array as [Any]:
            return !array.isEmpty
        default:
            return false
        }
    }

    /// Get child items for expansion
    private var childItems: [(key: String, value: Any?)] {
        // For functions, don't return child items (source is shown differently)
        if functionInfo != nil { return [] }
        switch value {
        case let dict as [String: Any]:
            return dict.keys.sorted().map { ($0, dict[$0]) }
        case let array as [Any]:
            return array.enumerated().map { ("[\($0.offset)]", $0.element) }
        default:
            return []
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(alignment: .top, spacing: 4) {
                // Expand/collapse button for expandable values
                if isExpandable {
                    Button(action: { isExpanded.toggle() }) {
                        Image(systemName: isExpanded ? "chevron.down" : "chevron.right")
                            .font(.system(size: 8, weight: .bold))
                            .foregroundColor(.secondary)
                            .frame(width: 10, height: 10)
                    }
                    .buttonStyle(.plain)
                } else {
                    Spacer().frame(width: 10)
                }

                Text(key)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundColor(.purple)
                    .textSelection(.enabled)

                Text("=")
                    .font(.system(size: 11))
                    .foregroundColor(.secondary)

                // Function props are clickable links to navigate to source
                if let fn = functionInfo {
                    FunctionLinkButton(
                        displayText: formatValue(value),
                        functionName: fn.name,
                        sourceFile: fn.sourceFile,
                        sourceLine: fn.sourceLine
                    )
                } else {
                    Text(formatValue(value, brief: isExpandable && isExpanded))
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundColor(colorForValue(value))
                        .lineLimit(isExpanded ? nil : 3)
                        .textSelection(.enabled)
                }

                Spacer()
            }

            // Expanded children (for objects/arrays)
            if isExpanded && !childItems.isEmpty {
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(Array(childItems.enumerated()), id: \.offset) { _, item in
                        DevToolsPropRowView(key: item.key, value: item.value, depth: depth + 1)
                            .padding(.leading, 14)
                    }
                }
            }
        }
    }

    private func formatValue(_ value: Any?, brief: Bool = false) -> String {
        guard let value = value else { return "null" }

        switch value {
        case let str as String:
            // Check for legacy function marker: [Function] or [Function: name]
            if str.hasPrefix("[Function") {
                // Extract name from [Function: name] format
                if str.hasPrefix("[Function: "), str.hasSuffix("]") {
                    let startIndex = str.index(str.startIndex, offsetBy: 11) // "[Function: ".count
                    let endIndex = str.index(before: str.endIndex) // before "]"
                    let name = String(str[startIndex..<endIndex])
                    return "ƒ \(name)()"
                }
                return "ƒ()"
            }
            return "\"\(str)\""
        // Note: Check Bool before NSNumber since Bool bridges to NSNumber
        case let num as NSNumber where CFBooleanGetTypeID() == CFGetTypeID(num):
            return num.boolValue ? "true" : "false"
        case let num as NSNumber:
            return "\(num)"
        case let array as [Any]:
            if brief {
                return "Array(\(array.count))"
            }
            if array.count <= 3 {
                return "[\(array.map { formatValue($0) }.joined(separator: ", "))]"
            }
            return "Array(\(array.count))"
        case let dict as [String: Any]:
            // Check for function object type
            if dict["__type"] as? String == "function" {
                let name = dict["name"] as? String ?? "anonymous"
                return "ƒ \(name)()"
            }
            if brief {
                return "Object(\(dict.count))"
            }
            if dict.count <= 2 {
                let items = dict.map { "\($0.key): \(formatValue($0.value))" }
                return "{\(items.joined(separator: ", "))}"
            }
            return "Object(\(dict.count))"
        default:
            return String(describing: value)
        }
    }

    private func colorForValue(_ value: Any?) -> Color {
        guard let value = value else { return .secondary }

        switch value {
        case let str as String:
            if str.hasPrefix("[Function") {
                return .blue
            }
            return .orange
        // Note: Check Bool before NSNumber since Bool bridges to NSNumber
        case let num as NSNumber where CFBooleanGetTypeID() == CFGetTypeID(num):
            return .purple
        case is NSNumber:
            return .cyan
        case is [Any]:
            return .green
        case let dict as [String: Any]:
            // Function objects are blue
            if dict["__type"] as? String == "function" {
                return .blue
            }
            return .green
        default:
            return .primary
        }
    }
}

/// Tree node view for component hierarchy
struct DevToolsTreeNodeView: View {
    let node: DevToolsComponentNode
    let depth: Int
    @ObservedObject var devTools: DevToolsManager

    /// Whether this node is expanded (from global state)
    private var isExpanded: Bool {
        // Default: expand if no explicit state set, or if explicitly expanded
        devTools.expandedNodeIds.isEmpty || devTools.isNodeExpanded(node.id)
    }

    /// Check if this node has only __TEXT__ children (inline text display)
    /// Returns (text, shouldInline) - shouldInline is true only for short single-line text
    private var textContentInfo: (text: String, shouldInline: Bool)? {
        // If all children are __TEXT__ nodes, combine their text content
        guard !node.children.isEmpty else { return nil }
        let textChildren = node.children.filter { $0.type == "__TEXT__" }
        guard textChildren.count == node.children.count else { return nil }

        // Extract text from each __TEXT__ child's props
        let texts = textChildren.compactMap { child -> String? in
            if let text = child.props["text"] as? String {
                return text
            }
            return nil
        }
        guard texts.count == textChildren.count else { return nil }

        let combinedText = texts.joined()
        // Inline only if: single text child, short text, no newlines
        let shouldInline = textChildren.count == 1 &&
                          combinedText.count <= 40 &&
                          !combinedText.contains("\n")
        return (combinedText, shouldInline)
    }

    /// For inline display (short single text)
    private var inlineTextContent: String? {
        guard let info = textContentInfo, info.shouldInline else { return nil }
        return info.text
    }

    /// For expanded display (multiple texts or long/multiline text)
    private var expandedTextLines: [String]? {
        guard let info = textContentInfo, !info.shouldInline else { return nil }
        // Split by newlines and filter empty lines
        return info.text.components(separatedBy: "\n").filter { !$0.isEmpty }
    }

    /// Check if this is a __TEXT__ node (should be hidden, parent shows its content)
    private var isTextNode: Bool {
        node.type == "__TEXT__"
    }

    /// Whether this node has displayable children (non-__TEXT__ or mixed)
    private var hasDisplayableChildren: Bool {
        !node.children.isEmpty && textContentInfo == nil
    }

    /// Whether this node has expanded text content (multiline or long)
    private var hasExpandedTextContent: Bool {
        expandedTextLines != nil
    }

    var body: some View {
        // Skip rendering __TEXT__ nodes - their content is shown inline in parent
        if isTextNode {
            EmptyView()
        } else {
            VStack(alignment: .leading, spacing: 0) {
                // Opening tag row
                HStack(spacing: 4) {
                    // Expand/collapse button (only for nodes with expandable content)
                    if hasDisplayableChildren || hasExpandedTextContent {
                        Button(action: { devTools.toggleNodeExpanded(node.id) }) {
                            Image(systemName: isExpanded ? "chevron.down" : "chevron.right")
                                .font(.system(size: 8, weight: .bold))
                                .foregroundColor(.secondary)
                        }
                        .buttonStyle(.plain)
                        .frame(width: 12)
                    } else {
                        Spacer()
                            .frame(width: 12)
                    }

                    // Tag content
                    if let text = inlineTextContent {
                        // Inline text (short): <Text>content</Text>
                        HStack(spacing: 0) {
                            Text("<\(node.type)>")
                                .font(.system(size: 11, design: .monospaced))
                                .foregroundColor(devTools.selectedNodeId == node.id ? .white : .accentColor)
                            Text(truncateText(text, maxLength: 30))
                                .font(.system(size: 11, design: .monospaced))
                                .foregroundColor(devTools.selectedNodeId == node.id ? .white.opacity(0.9) : .primary)
                                .textSelection(.enabled)
                            Text("</\(node.type)>")
                                .font(.system(size: 11, design: .monospaced))
                                .foregroundColor(devTools.selectedNodeId == node.id ? .white : .accentColor)
                        }
                    } else if node.children.isEmpty {
                        // Self-closing tag: <View />
                        Text("<\(node.type) />")
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundColor(devTools.selectedNodeId == node.id ? .white : .accentColor)
                    } else {
                        // Opening tag only: <View> or <Text> (for expanded text)
                        Text("<\(node.type)>")
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundColor(devTools.selectedNodeId == node.id ? .white : .accentColor)
                    }

                    Spacer()
                }
                .padding(.leading, CGFloat(depth) * 16)
                .padding(.vertical, 2)
                .padding(.horizontal, 4)
                .background(
                    RoundedRectangle(cornerRadius: 2)
                        .fill(devTools.selectedNodeId == node.id ? Color.accentColor : Color.clear)
                )
                .contentShape(Rectangle())
                .onTapGesture {
                    devTools.selectNode(node.id)
                }
                .id(node.id)  // For ScrollViewReader scrollTo

                // Expanded text content (multiline or long text)
                if isExpanded, let textLines = expandedTextLines {
                    let isSelected = devTools.selectedNodeId == node.id
                    ForEach(Array(textLines.enumerated()), id: \.offset) { _, line in
                        HStack(spacing: 4) {
                            Spacer()
                                .frame(width: 12)
                            Text(truncateText(line, maxLength: 50))
                                .font(.system(size: 11, design: .monospaced))
                                .foregroundColor(isSelected ? .white.opacity(0.9) : .primary.opacity(0.8))
                                .textSelection(.enabled)
                            Spacer()
                        }
                        .padding(.leading, CGFloat(depth + 1) * 16)
                        .padding(.vertical, 1)
                        .padding(.horizontal, 4)
                    }

                    // Closing tag for expanded text
                    HStack(spacing: 4) {
                        Spacer()
                            .frame(width: 12)
                        Text("</\(node.type)>")
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundColor(isSelected ? .white : .accentColor.opacity(0.7))
                        Spacer()
                    }
                    .padding(.leading, CGFloat(depth) * 16)
                    .padding(.vertical, 2)
                    .padding(.horizontal, 4)
                }

                // Regular children (only if expanded and has displayable children)
                if isExpanded && hasDisplayableChildren {
                    ForEach(node.children) { child in
                        DevToolsTreeNodeView(node: child, depth: depth + 1, devTools: devTools)
                    }

                    // Closing tag
                    HStack(spacing: 4) {
                        Spacer()
                            .frame(width: 12)
                        Text("</\(node.type)>")
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundColor(.accentColor.opacity(0.7))
                        Spacer()
                    }
                    .padding(.leading, CGFloat(depth) * 16)
                    .padding(.vertical, 2)
                    .padding(.horizontal, 4)
                }
            }
        }
    }

    /// Truncate text for display
    private func truncateText(_ text: String, maxLength: Int) -> String {
        if text.count <= maxLength {
            return text
        }
        return String(text.prefix(maxLength)) + "..."
    }
}

// MARK: - Console Tab

struct DevToolsConsoleView: View {
    @ObservedObject var devTools: DevToolsManager
    @State private var levelFilter: DevToolsLogEntry.LogLevel? = nil
    @State private var textFilter: String = ""

    private var filteredLogs: [DevToolsLogEntry] {
        devTools.consoleLogs.filter { entry in
            // Level filter
            if let level = levelFilter, entry.level != level {
                return false
            }
            // Text filter
            if !textFilter.isEmpty {
                let searchText = textFilter.lowercased()
                return entry.message.lowercased().contains(searchText) ||
                       entry.source.lowercased().contains(searchText)
            }
            return true
        }
    }

    var body: some View {
        VStack(spacing: 0) {
            // Filter bar
            HStack(spacing: 8) {
                // Level filter buttons
                HStack(spacing: 2) {
                    filterButton(nil, label: "All")
                    filterButton(.info, label: "Info")
                    filterButton(.warn, label: "Warn")
                    filterButton(.error, label: "Error")
                }

                // Text search
                HStack(spacing: 4) {
                    Image(systemName: "magnifyingglass")
                        .font(.system(size: 10))
                        .foregroundColor(.secondary)
                    TextField("Filter", text: $textFilter)
                        .textFieldStyle(.plain)
                        .font(.system(size: 11))
                }
                .padding(.horizontal, 6)
                .padding(.vertical, 3)
                .background(Color(nsColor: .textBackgroundColor))
                .cornerRadius(4)
                .frame(maxWidth: 140)

                Spacer()

                Text("\(filteredLogs.count) / \(devTools.consoleLogs.count)")
                    .font(.system(size: 10))
                    .foregroundColor(Color(nsColor: .tertiaryLabelColor))

                // Clear button
                Button(action: { devTools.clearConsoleLogs() }) {
                    Image(systemName: "trash")
                        .font(.system(size: 11))
                }
                .buttonStyle(.borderless)
                .help("Clear console")
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .background(Color(nsColor: .controlBackgroundColor))

            Divider()

            // Log list
            if filteredLogs.isEmpty {
                VStack {
                    if devTools.consoleLogs.isEmpty {
                        Text("No console output")
                            .font(.system(size: 12))
                            .foregroundColor(.secondary)
                    } else {
                        Text("No logs match filter")
                            .font(.system(size: 12))
                            .foregroundColor(.secondary)
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollViewReader { proxy in
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 0) {
                            ForEach(filteredLogs) { entry in
                                DevToolsLogEntryView(entry: entry)
                                    .id(entry.id)
                            }
                        }
                    }
                    .onChange(of: devTools.consoleLogs.count) { _, _ in
                        // Only auto-scroll if we were already near the bottom
                        // For now, simple auto-scroll to latest
                        if let last = filteredLogs.last {
                            proxy.scrollTo(last.id, anchor: .bottom)
                        }
                    }
                }
            }
        }
        .background(Color(nsColor: .textBackgroundColor))
    }

    private func filterButton(_ level: DevToolsLogEntry.LogLevel?, label: String) -> some View {
        Button(action: { levelFilter = level }) {
            Text(label)
                .font(.system(size: 10))
                .padding(.horizontal, 6)
                .padding(.vertical, 2)
                .background(levelFilter == level ? Color.accentColor : Color.clear)
                .foregroundColor(levelFilter == level ? .white : .secondary)
                .cornerRadius(3)
        }
        .buttonStyle(.plain)
    }
}

/// Single log entry view
struct DevToolsLogEntryView: View {
    let entry: DevToolsLogEntry

    private var levelColor: Color {
        switch entry.level {
        case .debug: return .gray
        case .info: return .primary
        case .warn: return .orange
        case .error: return .red
        }
    }

    private var levelIcon: String {
        switch entry.level {
        case .debug: return "ant"
        case .info: return "info.circle"
        case .warn: return "exclamationmark.triangle"
        case .error: return "xmark.circle"
        }
    }

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            // Timestamp
            Text(entry.timestamp, format: .dateTime.hour().minute().second())
                .font(.system(size: 10, design: .monospaced))
                .foregroundColor(Color(nsColor: .tertiaryLabelColor))
                .frame(width: 60, alignment: .leading)

            // Level icon
            Image(systemName: levelIcon)
                .font(.system(size: 10))
                .foregroundColor(levelColor)
                .frame(width: 14)

            // Source
            Text("[\(entry.source)]")
                .font(.system(size: 10, design: .monospaced))
                .foregroundColor(.secondary)
                .frame(width: 80, alignment: .leading)

            // Message
            Text(entry.message)
                .font(.system(size: 11, design: .monospaced))
                .foregroundColor(levelColor)
                .textSelection(.enabled)

            Spacer()
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 3)
        .background(entry.level == .error ? Color.red.opacity(0.1) : Color.clear)
    }
}

// MARK: - Network Tab

enum DevToolsNetworkFilter: String, CaseIterable, Identifiable {
    case all = "All"
    case pending = "Pending"
    case failed = "Failed"

    var id: String { rawValue }
}

struct DevToolsNetworkView: View {
    @ObservedObject var devTools: DevToolsManager
    @ObservedObject var viewModel: BrowserViewModel

    @State private var filter: DevToolsNetworkFilter = .all
    @State private var searchText: String = ""
    @State private var selectedRequestId: UUID?

    private var currentTabIdString: String? {
        viewModel.currentTabId?.uuidString
    }

    private var filteredRequests: [DevToolsNetworkRequest] {
        guard let currentTabIdString else { return [] }

        return devTools.networkRequests
            .filter { req in
                if req.tabId != currentTabIdString { return false }
                switch filter {
                case .all:
                    break
                case .pending:
                    if req.state != .pending { return false }
                case .failed:
                    if req.state != .failed { return false }
                }

                if !searchText.isEmpty {
                    let q = searchText.lowercased()
                    let matchesUrl = req.url.lowercased().contains(q)
                    let matchesMethod = req.method.lowercased().contains(q)
                    let matchesStatus = req.statusText.lowercased().contains(q)
                    if !(matchesUrl || matchesMethod || matchesStatus) { return false }
                }

                return true
            }
            .sorted { $0.startTime > $1.startTime }
    }

    private var selectedRequest: DevToolsNetworkRequest? {
        guard let selectedRequestId else { return nil }
        return devTools.networkRequests.first(where: { $0.id == selectedRequestId })
    }

    var body: some View {
        VStack(spacing: 0) {
            // Filter bar
            HStack(spacing: 8) {
                Picker("", selection: $filter) {
                    ForEach(DevToolsNetworkFilter.allCases) { option in
                        Text(option.rawValue).tag(option)
                    }
                }
                .pickerStyle(.segmented)
                .frame(width: 200)

                HStack(spacing: 4) {
                    Image(systemName: "magnifyingglass")
                        .font(.system(size: 10))
                        .foregroundColor(.secondary)
                    TextField("Filter", text: $searchText)
                        .textFieldStyle(.plain)
                        .font(.system(size: 11))
                }
                .padding(.horizontal, 6)
                .padding(.vertical, 3)
                .background(Color(nsColor: .textBackgroundColor))
                .cornerRadius(4)
                .frame(maxWidth: 180)

                Spacer()

                Text("\(filteredRequests.count) requests")
                    .font(.system(size: 10))
                    .foregroundColor(Color(nsColor: .tertiaryLabelColor))

                Button(action: { devTools.clearNetwork() }) {
                    Image(systemName: "trash")
                        .font(.system(size: 11))
                }
                .buttonStyle(.borderless)
                .help("Clear network")
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .background(Color(nsColor: .controlBackgroundColor))

            Divider()

            // Content
            if devTools.networkRequests.isEmpty {
                VStack(spacing: 8) {
                    Image(systemName: "network")
                        .font(.system(size: 24))
                        .foregroundColor(.secondary)
                    Text("No network requests")
                        .font(.system(size: 12))
                        .foregroundColor(.secondary)
                    Text("Requests will appear when Guest triggers APP_FETCH_*")
                        .font(.system(size: 11))
                        .foregroundColor(Color(nsColor: .tertiaryLabelColor))
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Color(nsColor: .textBackgroundColor))
            } else if filteredRequests.isEmpty {
                VStack(spacing: 8) {
                    Image(systemName: "line.3.horizontal.decrease.circle")
                        .font(.system(size: 24))
                        .foregroundColor(.secondary)
                    Text("No requests for current tab")
                        .font(.system(size: 12))
                        .foregroundColor(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Color(nsColor: .textBackgroundColor))
            } else {
                HSplitView {
                    // Request list
                    List(filteredRequests, selection: $selectedRequestId) { req in
                        DevToolsNetworkRowView(request: req)
                        .tag(req.id)
                    }
                    .listStyle(.plain)
                    .frame(minWidth: 420)

                    // Details
                    VStack(alignment: .leading, spacing: 0) {
                        if let req = selectedRequest {
                            DevToolsNetworkDetailView(request: req)
                        } else {
                            VStack {
                                Text("Select a request")
                                    .font(.system(size: 12))
                                    .foregroundColor(.secondary)
                            }
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                        }
                    }
                    .frame(minWidth: 280)
                }
                .background(Color(nsColor: .textBackgroundColor))
            }
        }
    }
}

private struct DevToolsNetworkRowView: View {
    let request: DevToolsNetworkRequest

    private var sizeText: String {
        guard let bytes = request.responseSizeBytes else { return "-" }
        if bytes < 1024 { return "\(bytes)B" }
        if bytes < 1024 * 1024 { return String(format: "%.1fKB", Double(bytes) / 1024.0) }
        return String(format: "%.1fMB", Double(bytes) / (1024.0 * 1024.0))
    }

    var body: some View {
        HStack(spacing: 8) {
            Text(request.method)
                .font(.system(size: 10, weight: .bold, design: .monospaced))
                .foregroundColor(.secondary)
                .frame(width: 45, alignment: .leading)

            VStack(alignment: .leading, spacing: 1) {
                Text(request.displayPathWithQuery)
                    .font(.system(size: 11))
                    .lineLimit(1)
                    .truncationMode(.middle)

                HStack(spacing: 6) {
                    if !request.displayHost.isEmpty {
                        Text(request.displayHost)
                            .font(.system(size: 9, design: .monospaced))
                            .foregroundColor(Color(nsColor: .tertiaryLabelColor))
                            .lineLimit(1)
                    }
                }
            }

            Spacer()

            Text(request.statusText)
                .font(.system(size: 10, weight: .medium))
                .foregroundColor(request.statusColor)
                .frame(width: 55, alignment: .trailing)

            Text(sizeText)
                .font(.system(size: 10, design: .monospaced))
                .foregroundColor(.secondary)
                .frame(width: 60, alignment: .trailing)

            if let duration = request.duration {
                Text(String(format: "%.0fms", duration * 1000))
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundColor(.secondary)
                    .frame(width: 60, alignment: .trailing)
            } else {
                ProgressView()
                    .scaleEffect(0.5)
                    .frame(width: 60, alignment: .trailing)
            }
        }
        .contextMenu {
            Button("Copy URL") {
                DevToolsClipboard.copy(request.url)
            }
            Button("Copy as cURL") {
                DevToolsClipboard.copy(DevToolsCurlBuilder.build(from: request))
            }
            Button("Open in Browser") {
                if let url = URL(string: request.url) {
                    NSWorkspace.shared.open(url)
                }
            }
        }
        .padding(.vertical, 2)
    }
}

private struct DevToolsNetworkDetailView: View {
    let request: DevToolsNetworkRequest

    private var queryItems: [(String, String)] {
        guard let components = URLComponents(string: request.url),
              let items = components.queryItems, !items.isEmpty else { return [] }
        return items.map { ($0.name, $0.value ?? "") }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Header
            VStack(alignment: .leading, spacing: 6) {
                HStack(alignment: .top, spacing: 8) {
                    Text(request.url)
                        .font(.system(size: 12, weight: .medium))
                        .textSelection(.enabled)
                        .lineLimit(2)

                    Spacer()

                    HStack(spacing: 6) {
                        Button {
                            DevToolsClipboard.copy(request.url)
                        } label: {
                            Image(systemName: "doc.on.doc")
                                .font(.system(size: 11))
                        }
                        .buttonStyle(.borderless)
                        .help("Copy URL")

                        Button {
                            DevToolsClipboard.copy(DevToolsCurlBuilder.build(from: request))
                        } label: {
                            Image(systemName: "terminal")
                                .font(.system(size: 11))
                        }
                        .buttonStyle(.borderless)
                        .help("Copy as cURL")

                        Button {
                            if let url = URL(string: request.url) {
                                NSWorkspace.shared.open(url)
                            }
                        } label: {
                            Image(systemName: "safari")
                                .font(.system(size: 11))
                        }
                        .buttonStyle(.borderless)
                        .help("Open in Browser")
                    }
                }

                HStack(spacing: 8) {
                    Text(request.method)
                        .font(.system(size: 11, weight: .bold, design: .monospaced))
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(Color.secondary.opacity(0.1))
                        .cornerRadius(3)

                    Text(request.statusText)
                        .font(.system(size: 11, weight: .bold))
                        .foregroundColor(request.statusColor)

                    if let duration = request.duration {
                        Text(String(format: "%.0fms", duration * 1000))
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundColor(.secondary)
                    }
                }
            }
            .padding(12)
            .background(Color(nsColor: .controlBackgroundColor))

            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    DevToolsSectionView(title: "General") {
                        DevToolsKeyValueView(items: [
                            ("Request ID", request.requestId),
                            ("URL", request.url),
                            ("Method", request.method),
                            ("Status", request.status.map(String.init) ?? "Pending"),
                            ("Start", request.startTime.formatted(.dateTime.hour().minute().second())),
                            ("Duration", request.duration.map { String(format: "%.0fms", $0 * 1000) } ?? "-"),
                        ])
                    }

                    if !queryItems.isEmpty {
                        DevToolsSectionView(title: "Query String") {
                            DevToolsKeyValueView(items: queryItems)
                        }
                    }

                    if let headers = request.requestHeaders, !headers.isEmpty {
                        DevToolsSectionView(title: "Request Headers") {
                            DevToolsHeadersView(headers: headers)
                        }
                    }

                    if let body = request.requestBody, !body.isEmpty {
                        DevToolsSectionView(title: "Request Body") {
                            DevToolsCodeBlockView(text: body)
                        }
                    }

                    if let headers = request.responseHeaders, !headers.isEmpty {
                        DevToolsSectionView(title: "Response Headers") {
                            DevToolsHeadersView(headers: headers)
                        }
                    }

                    if let body = request.responseBody, !body.isEmpty {
                        DevToolsSectionView(title: "Response Body") {
                            DevToolsCodeBlockView(text: body)
                        }
                    }

                    if let error = request.error, !error.isEmpty {
                        DevToolsSectionView(title: "Error") {
                            Text(error)
                                .font(.system(size: 11, design: .monospaced))
                                .foregroundColor(.red)
                                .textSelection(.enabled)
                                .padding(8)
                                .background(Color.red.opacity(0.08))
                                .cornerRadius(4)
                        }
                    }
                }
                .padding(12)
            }
        }
    }
}

private struct DevToolsSectionView<Content: View>: View {
    let title: String
    let content: Content

    init(title: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title)
                .font(.system(size: 11, weight: .bold))
                .foregroundColor(.secondary)
            content
        }
    }
}

private struct DevToolsHeadersView: View {
    let headers: [String: String]

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            ForEach(headers.keys.sorted(), id: \.self) { key in
                HStack(alignment: .top, spacing: 6) {
                    Text("\(key):")
                        .font(.system(size: 11, weight: .medium))
                        .foregroundColor(.secondary)
                        .frame(width: 140, alignment: .trailing)
                    Text(headers[key] ?? "")
                        .font(.system(size: 11, design: .monospaced))
                        .textSelection(.enabled)
                }
            }
        }
        .padding(8)
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.5))
        .cornerRadius(4)
    }
}

private struct DevToolsKeyValueView: View {
    let items: [(String, String)]

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            ForEach(Array(items.enumerated()), id: \.offset) { _, item in
                HStack(alignment: .top, spacing: 6) {
                    Text(item.0 + ":")
                        .font(.system(size: 11, weight: .medium))
                        .foregroundColor(.secondary)
                        .frame(width: 140, alignment: .trailing)
                    Text(item.1)
                        .font(.system(size: 11, design: .monospaced))
                        .textSelection(.enabled)
                }
            }
        }
        .padding(8)
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.5))
        .cornerRadius(4)
    }
}

private struct DevToolsCodeBlockView: View {
    let text: String

    var body: some View {
        ScrollView(.horizontal) {
            Text(text)
                .font(.system(size: 11, design: .monospaced))
                .foregroundColor(.primary)
                .textSelection(.enabled)
                .padding(8)
        }
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.5))
        .cornerRadius(4)
    }
}

private enum DevToolsClipboard {
    static func copy(_ string: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(string, forType: .string)
    }
}

private enum DevToolsCurlBuilder {
    static func build(from request: DevToolsNetworkRequest) -> String {
        var parts: [String] = ["curl"]
        parts.append("-X")
        parts.append(shellQuote(request.method))

        if let headers = request.requestHeaders {
            for key in headers.keys.sorted() {
                let value = headers[key] ?? ""
                parts.append("-H")
                parts.append(shellQuote("\(key): \(value)"))
            }
        }

        if let body = request.requestBody, !body.isEmpty, request.method.uppercased() != "GET" {
            parts.append("--data-raw")
            parts.append(shellQuote(body))
        }

        parts.append(shellQuote(request.url))
        return parts.joined(separator: " ")
    }

    private static func shellQuote(_ string: String) -> String {
        if string.isEmpty { return "''" }
        // POSIX shell single-quote escaping: ' -> '"'"'
        return "'" + string.replacingOccurrences(of: "'", with: "'\"'\"'") + "'"
    }
}

// MARK: - Messages Tab

struct DevToolsMessagesView: View {
    @ObservedObject var devTools: DevToolsManager
    @State private var selectedMessageId: UUID?
    @State private var showPayload: Bool = true

    var body: some View {
        VStack(spacing: 0) {
            // Filter bar
            HStack(spacing: 8) {
                // Category filter buttons
                HStack(spacing: 2) {
                    categoryFilterButton(nil, label: "All")
                    categoryFilterButton(.render, label: "Render")
                    categoryFilterButton(.app, label: "App")
                }

                // Direction filter buttons
                HStack(spacing: 2) {
                    directionFilterButton(nil, label: "All")
                    directionFilterButton(.guestToHost, label: "G↓H")
                    directionFilterButton(.hostToGuest, label: "H↑G")
                }

                // Search filter
                HStack(spacing: 4) {
                    Image(systemName: "magnifyingglass")
                        .font(.system(size: 10))
                        .foregroundColor(.secondary)
                    TextField("Filter", text: $devTools.messageFilter)
                        .textFieldStyle(.plain)
                        .font(.system(size: 11))
                }
                .padding(.horizontal, 6)
                .padding(.vertical, 3)
                .background(Color(nsColor: .textBackgroundColor))
                .cornerRadius(4)
                .frame(maxWidth: 140)

                Spacer()

                Text("\(devTools.filteredMessages.count) / \(devTools.messages.count)")
                    .font(.system(size: 10))
                    .foregroundColor(Color(nsColor: .tertiaryLabelColor))

                // Clear button
                Button(action: { devTools.clearMessages() }) {
                    Image(systemName: "trash")
                        .font(.system(size: 11))
                }
                .buttonStyle(.borderless)
                .help("Clear messages")
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .background(Color(nsColor: .controlBackgroundColor))

            Divider()

            // Messages list
            if devTools.messages.isEmpty {
                VStack(spacing: 8) {
                    Image(systemName: "arrow.left.arrow.right")
                        .font(.system(size: 24))
                        .foregroundColor(.secondary)
                    Text("No Host-Guest messages")
                        .font(.system(size: 12))
                        .foregroundColor(.secondary)
                    Text("Messages will appear when Guest and Host communicate")
                        .font(.system(size: 11))
                        .foregroundColor(Color(nsColor: .tertiaryLabelColor))
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if devTools.filteredMessages.isEmpty {
                VStack(spacing: 8) {
                    Image(systemName: "line.3.horizontal.decrease.circle")
                        .font(.system(size: 24))
                        .foregroundColor(.secondary)
                    Text("No messages match filter")
                        .font(.system(size: 12))
                        .foregroundColor(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                HSplitView {
                    // Message list (left)
                    ScrollViewReader { proxy in
                        ScrollView {
                            LazyVStack(alignment: .leading, spacing: 0) {
                                ForEach(devTools.filteredMessages) { msg in
                                    DevToolsMessageRowView(
                                        message: msg,
                                        isSelected: selectedMessageId == msg.id
                                    )
                                    .id(msg.id)
                                    .onTapGesture {
                                        selectedMessageId = msg.id
                                    }
                                }
                            }
                        }
                        .onChange(of: devTools.messages.count) { _, _ in
                            // Auto-scroll to latest if no selection or if we want to follow tail
                            // For now, simple auto-scroll
                            if let last = devTools.filteredMessages.last {
                                proxy.scrollTo(last.id, anchor: .bottom)
                            }
                        }
                    }
                    .frame(minWidth: 400)

                    // Payload detail (right)
                    VStack(alignment: .leading, spacing: 0) {
                        HStack {
                            Text("Payload")
                                .font(.system(size: 11, weight: .semibold))
                                .foregroundColor(.secondary)
                            Spacer()
                        }
                        .padding(.horizontal, 8)
                        .padding(.vertical, 6)
                        .background(Color(nsColor: .controlBackgroundColor))

                        Divider()

                        if let msgId = selectedMessageId,
                           let msg = devTools.messages.first(where: { $0.id == msgId }) {
                            ScrollView {
                                VStack(alignment: .leading, spacing: 8) {
                                    // Event info
                                    HStack(spacing: 4) {
                                        Text(msg.directionArrow)
                                            .foregroundColor(msg.direction == .guestToHost ? .blue : .green)
                                        Text(msg.event)
                                            .font(.system(size: 12, weight: .semibold, design: .monospaced))
                                    }

                                    if let tabId = msg.tabId {
                                        Text("Tab: \(tabId)")
                                            .font(.system(size: 10, design: .monospaced))
                                            .foregroundColor(.secondary)
                                    }

                                    Divider()

                                    // Payload JSON
                                    if let payload = msg.payload {
                                        Text(payload)
                                            .font(.system(size: 11, design: .monospaced))
                                            .foregroundColor(.primary)
                                            .textSelection(.enabled)
                                    } else {
                                        Text("(no payload)")
                                            .font(.system(size: 11))
                                            .foregroundColor(.secondary)
                                            .italic()
                                    }
                                }
                                .padding(8)
                            }
                        } else {
                            VStack {
                                Text("Select a message")
                                    .font(.system(size: 12))
                                    .foregroundColor(.secondary)
                            }
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                        }
                    }
                    .frame(minWidth: 250)
                }
            }
        }
        .background(Color(nsColor: .textBackgroundColor))
    }

    private func categoryFilterButton(_ category: MessageCategory?, label: String) -> some View {
        Button(action: { devTools.messageCategoryFilter = category }) {
            Text(label)
                .font(.system(size: 10))
                .padding(.horizontal, 6)
                .padding(.vertical, 2)
                .background(devTools.messageCategoryFilter == category ? Color.accentColor : Color.clear)
                .foregroundColor(devTools.messageCategoryFilter == category ? .white : .secondary)
                .cornerRadius(3)
        }
        .buttonStyle(.plain)
    }

    private func directionFilterButton(_ direction: MessageDirection?, label: String) -> some View {
        Button(action: { devTools.messageDirectionFilter = direction }) {
            Text(label)
                .font(.system(size: 10))
                .padding(.horizontal, 6)
                .padding(.vertical, 2)
                .background(devTools.messageDirectionFilter == direction ? Color.accentColor : Color.clear)
                .foregroundColor(devTools.messageDirectionFilter == direction ? .white : .secondary)
                .cornerRadius(3)
        }
        .buttonStyle(.plain)
    }
}

/// Single message row view
struct DevToolsMessageRowView: View {
    let message: DevToolsMessage
    let isSelected: Bool

    private var categoryColor: Color {
        message.category == .render ? .orange : .purple
    }

    var body: some View {
        HStack(alignment: .center, spacing: 6) {
            // Timestamp
            Text(message.timestamp, format: .dateTime.hour().minute().second().secondFraction(.fractional(3)))
                .font(.system(size: 10, design: .monospaced))
                .foregroundColor(Color(nsColor: .tertiaryLabelColor))
                .frame(width: 85, alignment: .leading)

            // Category icon
            Image(systemName: message.category.icon)
                .font(.system(size: 9))
                .foregroundColor(isSelected ? .white : categoryColor)
                .frame(width: 14)

            // Direction arrow (only for app messages or if direction matters)
            Text(message.directionArrow)
                .font(.system(size: 11, weight: .bold))
                .foregroundColor(isSelected ? .white : (message.direction == .guestToHost ? .blue : .green))
                .frame(width: 14)

            // Event name
            Text(message.event)
                .font(.system(size: 11, weight: .medium, design: .monospaced))
                .foregroundColor(isSelected ? .white : .primary)
                .frame(width: 70, alignment: .leading)

            // Component type (for render) or detail
            if let componentType = message.componentType {
                Text(componentType)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundColor(isSelected ? .white.opacity(0.9) : .accentColor)
                    .lineLimit(1)
                    .frame(width: 80, alignment: .leading)
            }

            // Node ID (for render)
            if let nodeId = message.nodeId {
                Text(nodeId)
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundColor(isSelected ? .white.opacity(0.7) : .secondary)
                    .lineLimit(1)
                    .frame(width: 60, alignment: .leading)
            }

            Spacer()
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 3)
        .background(
            RoundedRectangle(cornerRadius: 2)
                .fill(isSelected ? Color.accentColor : Color.clear)
        )
        .contentShape(Rectangle())
    }
}

// MARK: - Box Model View

/// Visual box model diagram showing margin, border, padding, and content
struct DevToolsBoxModelView: View {
    let boxModel: DevToolsBoxModel

    // Colors matching Chrome DevTools
    private let marginColor = Color.orange.opacity(0.3)
    private let borderColor = Color.yellow.opacity(0.4)
    private let paddingColor = Color.green.opacity(0.3)
    private let contentColor = Color.blue.opacity(0.3)

    var body: some View {
        VStack(spacing: 8) {
            // Position info
            HStack(spacing: 12) {
                Text("position")
                    .font(.system(size: 10))
                    .foregroundColor(.secondary)
                Text("(\(Int(boxModel.x)), \(Int(boxModel.y)))")
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundColor(.primary)
            }

            // Box model diagram
            ZStack {
                // Margin box (outermost)
                RoundedRectangle(cornerRadius: 4)
                    .fill(marginColor)

                VStack(spacing: 0) {
                    // Top margin label
                    Text(formatValue(boxModel.margin.top))
                        .font(.system(size: 9, design: .monospaced))
                        .foregroundColor(.white)
                        .frame(height: 16)

                    HStack(spacing: 0) {
                        // Left margin label
                        Text(formatValue(boxModel.margin.left))
                            .font(.system(size: 9, design: .monospaced))
                            .foregroundColor(.white)
                            .frame(width: 24)

                        // Border box
                        ZStack {
                            RoundedRectangle(cornerRadius: 3)
                                .fill(borderColor)

                            VStack(spacing: 0) {
                                // Top border label
                                Text(formatValue(boxModel.border.top))
                                    .font(.system(size: 8, design: .monospaced))
                                    .foregroundColor(.white)
                                    .frame(height: 12)

                                HStack(spacing: 0) {
                                    // Left border label
                                    Text(formatValue(boxModel.border.left))
                                        .font(.system(size: 8, design: .monospaced))
                                        .foregroundColor(.white)
                                        .frame(width: 16)

                                    // Padding box
                                    ZStack {
                                        RoundedRectangle(cornerRadius: 2)
                                            .fill(paddingColor)

                                        VStack(spacing: 0) {
                                            // Top padding label
                                            Text(formatValue(boxModel.padding.top))
                                                .font(.system(size: 9, design: .monospaced))
                                                .foregroundColor(.white)
                                                .frame(height: 14)

                                            HStack(spacing: 0) {
                                                // Left padding label
                                                Text(formatValue(boxModel.padding.left))
                                                    .font(.system(size: 9, design: .monospaced))
                                                    .foregroundColor(.white)
                                                    .frame(width: 20)

                                                // Content box (innermost)
                                                ZStack {
                                                    RoundedRectangle(cornerRadius: 1)
                                                        .fill(contentColor)

                                                    Text("\(Int(boxModel.width)) × \(Int(boxModel.height))")
                                                        .font(.system(size: 10, weight: .medium, design: .monospaced))
                                                        .foregroundColor(.white)
                                                }
                                                .frame(minWidth: 60, minHeight: 30)

                                                // Right padding label
                                                Text(formatValue(boxModel.padding.right))
                                                    .font(.system(size: 9, design: .monospaced))
                                                    .foregroundColor(.white)
                                                    .frame(width: 20)
                                            }

                                            // Bottom padding label
                                            Text(formatValue(boxModel.padding.bottom))
                                                .font(.system(size: 9, design: .monospaced))
                                                .foregroundColor(.white)
                                                .frame(height: 14)
                                        }
                                    }

                                    // Right border label
                                    Text(formatValue(boxModel.border.right))
                                        .font(.system(size: 8, design: .monospaced))
                                        .foregroundColor(.white)
                                        .frame(width: 16)
                                }

                                // Bottom border label
                                Text(formatValue(boxModel.border.bottom))
                                    .font(.system(size: 8, design: .monospaced))
                                    .foregroundColor(.white)
                                    .frame(height: 12)
                            }
                        }

                        // Right margin label
                        Text(formatValue(boxModel.margin.right))
                            .font(.system(size: 9, design: .monospaced))
                            .foregroundColor(.white)
                            .frame(width: 24)
                    }

                    // Bottom margin label
                    Text(formatValue(boxModel.margin.bottom))
                        .font(.system(size: 9, design: .monospaced))
                        .foregroundColor(.white)
                        .frame(height: 16)
                }
            }
            .frame(height: 140)

            // Legend
            HStack(spacing: 12) {
                LegendItem(color: marginColor, label: "margin")
                LegendItem(color: borderColor, label: "border")
                LegendItem(color: paddingColor, label: "padding")
                LegendItem(color: contentColor, label: "content")
            }
            .padding(.top, 4)
        }
        .padding(8)
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.5))
        .cornerRadius(6)
    }

    private func formatValue(_ value: CGFloat) -> String {
        value == 0 ? "-" : "\(Int(value))"
    }
}

/// Legend item for box model colors
struct LegendItem: View {
    let color: Color
    let label: String

    var body: some View {
        HStack(spacing: 4) {
            RoundedRectangle(cornerRadius: 2)
                .fill(color)
                .frame(width: 10, height: 10)
            Text(label)
                .font(.system(size: 9))
                .foregroundColor(.secondary)
        }
    }
}

/// Computed styles section view
struct DevToolsComputedStylesView: View {
    let styles: DevToolsComputedStyles

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Computed Styles")
                .font(.system(size: 10, weight: .semibold))
                .foregroundColor(.secondary)

            VStack(alignment: .leading, spacing: 4) {
                if let position = styles.position {
                    StyleRow(name: "position", value: position)
                }
                if let backgroundColor = styles.backgroundColor {
                    StyleRow(name: "backgroundColor", value: backgroundColor)
                }
                if let display = styles.display {
                    StyleRow(name: "display", value: display)
                }
                if let flexDirection = styles.flexDirection {
                    StyleRow(name: "flexDirection", value: flexDirection)
                }
                if let alignItems = styles.alignItems {
                    StyleRow(name: "alignItems", value: alignItems)
                }
                if let justifyContent = styles.justifyContent {
                    StyleRow(name: "justifyContent", value: justifyContent)
                }
                if let flex = styles.flex {
                    StyleRow(name: "flex", value: "\(flex)")
                }
                if let opacity = styles.opacity {
                    StyleRow(name: "opacity", value: String(format: "%.2f", opacity))
                }
                if let overflow = styles.overflow {
                    StyleRow(name: "overflow", value: overflow)
                }
                if let top = styles.top {
                    StyleRow(name: "top", value: "\(Int(top))")
                }
                if let left = styles.left {
                    StyleRow(name: "left", value: "\(Int(left))")
                }
                if let right = styles.right {
                    StyleRow(name: "right", value: "\(Int(right))")
                }
                if let bottom = styles.bottom {
                    StyleRow(name: "bottom", value: "\(Int(bottom))")
                }
            }
        }
        .padding(8)
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.5))
        .cornerRadius(6)
    }
}

/// Single style row
struct StyleRow: View {
    let name: String
    let value: String

    var body: some View {
        HStack(spacing: 4) {
            Text(name)
                .font(.system(size: 10, design: .monospaced))
                .foregroundColor(.purple)
            Text(":")
                .font(.system(size: 10))
                .foregroundColor(.secondary)
            Text(value)
                .font(.system(size: 10, design: .monospaced))
                .foregroundColor(.primary)
            Spacer()
        }
    }
}

// MARK: - Function Link Button

/// View that displays function - clickable link only when source location is available
struct FunctionLinkButton: View {
    let displayText: String
    let functionName: String
    let sourceFile: String?  // Injected by Babel plugin
    let sourceLine: Int?     // Injected by Babel plugin

    @State private var isHovered = false

    /// Check if this function has source location (only then make it clickable)
    private var hasSourceLocation: Bool {
        sourceFile != nil
    }

    var body: some View {
        if hasSourceLocation {
            // Clickable link with hover underline
            Button(action: {
                DevToolsManager.shared.navigateToSource(
                    functionName: functionName,
                    fileName: sourceFile,
                    lineNumber: sourceLine
                )
            }) {
                Text(displayText)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundColor(.blue)
                    .underline(isHovered)
            }
            .buttonStyle(.plain)
            .onHover { hovering in
                isHovered = hovering
            }
            .help("Click to open \(sourceFile!):\(sourceLine ?? 0)")
        } else {
            // Plain text when no source location available
            Text(displayText)
                .font(.system(size: 11, design: .monospaced))
                .foregroundColor(.blue)
        }
    }
}

// MARK: - JavaScript Source View

/// View that displays JavaScript source code with formatting and syntax highlighting
struct JavaScriptSourceView: View {
    let source: String

    private static let highlightr: Highlightr? = {
        let h = Highlightr()
        h?.setTheme(to: "atom-one-dark")
        return h
    }()

    private var beautifiedSource: String {
        JSBeautifier.shared.beautify(source)
    }

    var body: some View {
        let formatted = beautifiedSource
        if let highlightr = Self.highlightr,
           let highlighted = highlightr.highlight(formatted, as: "javascript") {
            Text(AttributedString(highlighted))
                .font(.system(size: 10, design: .monospaced))
                .textSelection(.enabled)
        } else {
            // Fallback if highlighting fails
            Text(formatted)
                .font(.system(size: 10, design: .monospaced))
                .foregroundColor(.green.opacity(0.9))
                .textSelection(.enabled)
        }
    }
}

// MARK: - Preview

#Preview("DevTools Panel") {
    DevToolsPanel(
        devTools: DevToolsManager.shared,
        viewModel: BrowserViewModel()
    )
    .frame(width: 800, height: 300)
}

#Preview("Box Model") {
    DevToolsBoxModelView(boxModel: DevToolsBoxModel(
        margin: .init(top: 8, right: 16, bottom: 8, left: 16),
        border: .init(top: 1, right: 1, bottom: 1, left: 1),
        padding: .init(top: 12, right: 20, bottom: 12, left: 20),
        width: 200,
        height: 100,
        x: 50,
        y: 120
    ))
    .frame(width: 300)
    .padding()
}
