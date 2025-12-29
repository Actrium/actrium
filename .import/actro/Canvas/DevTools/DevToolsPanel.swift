import SwiftUI

/// Main DevTools panel view - used both for docked mode and detached window
struct DevToolsPanel: View {
    @ObservedObject var devTools: DevToolsManager
    @ObservedObject var viewModel: BrowserViewModel

    /// Whether this panel is in detached window mode
    var isDetached: Bool = false

    var body: some View {
        VStack(spacing: 0) {
            // Toolbar
            DevToolsToolbar(
                devTools: devTools,
                viewModel: viewModel,
                isDetached: isDetached
            )

            Divider()

            // Content based on active tab
            DevToolsContent(devTools: devTools, viewModel: viewModel)
        }
        .background(Color(nsColor: .windowBackgroundColor))
    }
}

/// DevTools toolbar with tab buttons and controls
struct DevToolsToolbar: View {
    @ObservedObject var devTools: DevToolsManager
    @ObservedObject var viewModel: BrowserViewModel
    var isDetached: Bool
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        HStack(spacing: 0) {
            // Tab buttons
            HStack(spacing: 2) {
                ForEach(DevToolsTab.allCases) { tab in
                    DevToolsTabButton(
                        tab: tab,
                        isSelected: devTools.activeTab == tab
                    ) {
                        devTools.activeTab = tab
                    }
                }
            }
            .padding(.horizontal, 8)

            Spacer()

            // Pause button and current inspected tab indicator
            if let currentTab = viewModel.currentTab {
                HStack(spacing: 6) {
                    // Pause/Resume button
                    Button(action: {
                        devTools.togglePause()
                    }) {
                        HStack(spacing: 4) {
                            Circle()
                                .fill(devTools.isEnginePaused ? Color.yellow : Color.green)
                                .frame(width: 8, height: 8)
                            Image(systemName: devTools.isEnginePaused ? "play.fill" : "pause.fill")
                                .font(.system(size: 10))
                                .foregroundColor(devTools.isEnginePaused ? .yellow : .secondary)
                        }
                    }
                    .buttonStyle(.borderless)
                    .help(devTools.isEnginePaused ? "Resume Engine" : "Pause Engine")

                    Text(currentTab.title)
                        .font(.system(size: 11))
                        .foregroundColor(.secondary)
                        .lineLimit(1)
                }
                .padding(.horizontal, 8)
            }

            Spacer()

            // Action buttons
            HStack(spacing: 4) {
                // Dock position toggle (only when docked)
                if !isDetached {
                    Button(action: {
                        devTools.toggleDockPosition()
                    }) {
                        Image(systemName: devTools.dockPosition == .bottom ? "rectangle.righthalf.inset.filled" : "rectangle.bottomhalf.inset.filled")
                            .font(.system(size: 12))
                    }
                    .buttonStyle(.borderless)
                    .help(devTools.dockPosition == .bottom ? "Dock to Right" : "Dock to Bottom")
                }

                // Detach/Dock button
                Button(action: {
                    if isDetached {
                        devTools.dock()
                    } else {
                        devTools.detach()
                        openWindow(id: "devtools")
                    }
                }) {
                    Image(systemName: isDetached ? "rectangle.inset.bottomright.filled" : "rectangle.on.rectangle")
                        .font(.system(size: 12))
                }
                .buttonStyle(.borderless)
                .help(isDetached ? "Dock to Main Window" : "Open in Separate Window")

                // Close button
                Button(action: {
                    devTools.hide()
                }) {
                    Image(systemName: "xmark")
                        .font(.system(size: 11, weight: .medium))
                }
                .buttonStyle(.borderless)
                .help("Close DevTools")
            }
            .padding(.horizontal, 8)
        }
        .frame(height: 32)
        .background(Color(nsColor: .controlBackgroundColor))
    }
}

/// Individual tab button in the toolbar
struct DevToolsTabButton: View {
    let tab: DevToolsTab
    let isSelected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 4) {
                Image(systemName: tab.icon)
                    .font(.system(size: 11))
                Text(tab.rawValue)
                    .font(.system(size: 11, weight: isSelected ? .semibold : .regular))
            }
            .foregroundColor(isSelected ? .accentColor : .secondary)
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .background(
                RoundedRectangle(cornerRadius: 4)
                    .fill(isSelected ? Color.accentColor.opacity(0.15) : Color.clear)
            )
        }
        .buttonStyle(.plain)
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
            case .console:
                DevToolsConsoleView(devTools: devTools)
            case .operations:
                DevToolsOperationsView(devTools: devTools, viewModel: viewModel)
            case .network:
                DevToolsPlaceholderView(title: "Network", message: "Network inspector coming soon")
            case .performance:
                DevToolsPlaceholderView(title: "Performance", message: "Performance profiler coming soon")
            }
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

                    // RN's built-in Element Inspector button
                    Button(action: { devTools.toggleRNInspector() }) {
                        HStack(spacing: 4) {
                            Image(systemName: "eye")
                                .font(.system(size: 10))
                            Text("RN")
                                .font(.system(size: 9, weight: .medium))
                        }
                        .foregroundColor(devTools.isRNInspectorActive ? .accentColor : .secondary)
                    }
                    .buttonStyle(.borderless)
                    .help("Toggle React Native's built-in Element Inspector")

                    // Node count
                    if let tree = devTools.componentTree {
                        Text("\(countNodes(tree))")
                            .font(.system(size: 10))
                            .foregroundColor(Color(nsColor: .tertiaryLabelColor))
                    }
                }
                .padding(.horizontal, 8)
                .padding(.vertical, 6)
                .background(Color(nsColor: .controlBackgroundColor))

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
                .padding(.vertical, 6)
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
                                Text("#\(node.id)")
                                    .font(.system(size: 11, design: .monospaced))
                                    .foregroundColor(.secondary)
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

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            Text(key)
                .font(.system(size: 11, design: .monospaced))
                .foregroundColor(.purple)
                .frame(minWidth: 60, alignment: .leading)

            Text("=")
                .font(.system(size: 11))
                .foregroundColor(.secondary)

            Text(formatValue(value))
                .font(.system(size: 11, design: .monospaced))
                .foregroundColor(colorForValue(value))
                .lineLimit(3)
                .textSelection(.enabled)

            Spacer()
        }
    }

    private func formatValue(_ value: Any?) -> String {
        guard let value = value else { return "null" }

        switch value {
        case let str as String:
            if str == "[Function]" {
                return "ƒ()"
            }
            return "\"\(str)\""
        // Note: Check Bool before NSNumber since Bool bridges to NSNumber
        case let num as NSNumber where CFBooleanGetTypeID() == CFGetTypeID(num):
            return num.boolValue ? "true" : "false"
        case let num as NSNumber:
            return "\(num)"
        case let array as [Any]:
            if array.count <= 3 {
                return "[\(array.map { formatValue($0) }.joined(separator: ", "))]"
            }
            return "Array(\(array.count))"
        case let dict as [String: Any]:
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
            if str == "[Function]" {
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
        case is [String: Any]:
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

    var body: some View {
        VStack(spacing: 0) {
            // Filter bar
            HStack {
                Image(systemName: "line.3.horizontal.decrease.circle")
                    .font(.system(size: 11))
                    .foregroundColor(.secondary)
                Text("All Levels")
                    .font(.system(size: 11))
                    .foregroundColor(.secondary)
                Spacer()
                Text("\(devTools.consoleLogs.count) entries")
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
            if devTools.consoleLogs.isEmpty {
                VStack {
                    Text("No console output")
                        .font(.system(size: 12))
                        .foregroundColor(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollViewReader { proxy in
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 0) {
                            ForEach(devTools.consoleLogs) { entry in
                                DevToolsLogEntryView(entry: entry)
                            }
                        }
                    }
                }
            }
        }
        .background(Color(nsColor: .textBackgroundColor))
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

// MARK: - Operations Tab

struct DevToolsOperationsView: View {
    @ObservedObject var devTools: DevToolsManager
    @ObservedObject var viewModel: BrowserViewModel

    var body: some View {
        VStack(spacing: 0) {
            // Filter bar
            HStack {
                Image(systemName: "line.3.horizontal.decrease.circle")
                    .font(.system(size: 11))
                    .foregroundColor(.secondary)
                Text("All Operations")
                    .font(.system(size: 11))
                    .foregroundColor(.secondary)
                Spacer()
                Text("\(devTools.operations.count) ops")
                    .font(.system(size: 10))
                    .foregroundColor(Color(nsColor: .tertiaryLabelColor))

                // Clear button
                Button(action: { devTools.clearOperations() }) {
                    Image(systemName: "trash")
                        .font(.system(size: 11))
                }
                .buttonStyle(.borderless)
                .help("Clear operations")
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .background(Color(nsColor: .controlBackgroundColor))

            Divider()

            // Operations list
            if devTools.operations.isEmpty {
                VStack {
                    Text("No render operations recorded")
                        .font(.system(size: 12))
                        .foregroundColor(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        ForEach(devTools.operations) { op in
                            DevToolsOperationEntryView(operation: op)
                        }
                    }
                }
            }
        }
        .background(Color(nsColor: .textBackgroundColor))
    }
}

/// Single operation entry view
struct DevToolsOperationEntryView: View {
    let operation: DevToolsOperation

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            // Timestamp
            Text(operation.timestamp, format: .dateTime.hour().minute().second())
                .font(.system(size: 10, design: .monospaced))
                .foregroundColor(Color(nsColor: .tertiaryLabelColor))
                .frame(width: 60, alignment: .leading)

            // Operation type badge
            Text(operation.operationType)
                .font(.system(size: 9, weight: .semibold, design: .monospaced))
                .foregroundColor(.white)
                .padding(.horizontal, 6)
                .padding(.vertical, 2)
                .background(operationColor)
                .cornerRadius(3)

            // Component type
            if let componentType = operation.componentType {
                Text("<\(componentType)>")
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundColor(.accentColor)
            }

            // Duration
            if let duration = operation.duration {
                Text(String(format: "%.2fms", duration * 1000))
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundColor(.secondary)
            }

            Spacer()
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 3)
    }

    private var operationColor: Color {
        switch operation.operationType.lowercased() {
        case "create": return .green
        case "update": return .blue
        case "delete": return .red
        case "mount": return .purple
        default: return .gray
        }
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
                // Category filter
                Picker("", selection: $devTools.messageCategoryFilter) {
                    Text("All").tag(nil as MessageCategory?)
                    Label("Render", systemImage: "hammer").tag(MessageCategory.render as MessageCategory?)
                    Label("App", systemImage: "envelope").tag(MessageCategory.app as MessageCategory?)
                }
                .pickerStyle(.segmented)
                .frame(width: 180)

                // Direction filter
                Picker("", selection: $devTools.messageDirectionFilter) {
                    Text("All").tag(nil as MessageDirection?)
                    Text("G ↓ H").tag(MessageDirection.guestToHost as MessageDirection?)
                    Text("H ↑ G").tag(MessageDirection.hostToGuest as MessageDirection?)
                }
                .pickerStyle(.segmented)
                .frame(width: 160)

                // Search filter
                HStack(spacing: 4) {
                    Image(systemName: "magnifyingglass")
                        .font(.system(size: 11))
                        .foregroundColor(.secondary)
                    TextField("Filter...", text: $devTools.messageFilter)
                        .textFieldStyle(.plain)
                        .font(.system(size: 11))
                        .frame(maxWidth: 150)
                }
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .background(Color(nsColor: .textBackgroundColor))
                .cornerRadius(4)

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
            .padding(.vertical, 6)
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
                                    .onTapGesture {
                                        selectedMessageId = msg.id
                                    }
                                }
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

            // Payload preview
            if let payload = message.payload {
                Text(payload.prefix(40).replacingOccurrences(of: "\n", with: " "))
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundColor(isSelected ? .white.opacity(0.7) : .secondary)
                    .lineLimit(1)
                    .frame(maxWidth: 150, alignment: .trailing)
            }
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
                        .foregroundColor(.orange)
                        .frame(height: 16)

                    HStack(spacing: 0) {
                        // Left margin label
                        Text(formatValue(boxModel.margin.left))
                            .font(.system(size: 9, design: .monospaced))
                            .foregroundColor(.orange)
                            .frame(width: 24)

                        // Border box
                        ZStack {
                            RoundedRectangle(cornerRadius: 3)
                                .fill(borderColor)

                            VStack(spacing: 0) {
                                // Top border label
                                Text(formatValue(boxModel.border.top))
                                    .font(.system(size: 8, design: .monospaced))
                                    .foregroundColor(.yellow)
                                    .frame(height: 12)

                                HStack(spacing: 0) {
                                    // Left border label
                                    Text(formatValue(boxModel.border.left))
                                        .font(.system(size: 8, design: .monospaced))
                                        .foregroundColor(.yellow)
                                        .frame(width: 16)

                                    // Padding box
                                    ZStack {
                                        RoundedRectangle(cornerRadius: 2)
                                            .fill(paddingColor)

                                        VStack(spacing: 0) {
                                            // Top padding label
                                            Text(formatValue(boxModel.padding.top))
                                                .font(.system(size: 9, design: .monospaced))
                                                .foregroundColor(.green)
                                                .frame(height: 14)

                                            HStack(spacing: 0) {
                                                // Left padding label
                                                Text(formatValue(boxModel.padding.left))
                                                    .font(.system(size: 9, design: .monospaced))
                                                    .foregroundColor(.green)
                                                    .frame(width: 20)

                                                // Content box (innermost)
                                                ZStack {
                                                    RoundedRectangle(cornerRadius: 1)
                                                        .fill(contentColor)

                                                    Text("\(Int(boxModel.width)) × \(Int(boxModel.height))")
                                                        .font(.system(size: 10, weight: .medium, design: .monospaced))
                                                        .foregroundColor(.blue)
                                                }
                                                .frame(minWidth: 60, minHeight: 30)

                                                // Right padding label
                                                Text(formatValue(boxModel.padding.right))
                                                    .font(.system(size: 9, design: .monospaced))
                                                    .foregroundColor(.green)
                                                    .frame(width: 20)
                                            }

                                            // Bottom padding label
                                            Text(formatValue(boxModel.padding.bottom))
                                                .font(.system(size: 9, design: .monospaced))
                                                .foregroundColor(.green)
                                                .frame(height: 14)
                                        }
                                    }

                                    // Right border label
                                    Text(formatValue(boxModel.border.right))
                                        .font(.system(size: 8, design: .monospaced))
                                        .foregroundColor(.yellow)
                                        .frame(width: 16)
                                }

                                // Bottom border label
                                Text(formatValue(boxModel.border.bottom))
                                    .font(.system(size: 8, design: .monospaced))
                                    .foregroundColor(.yellow)
                                    .frame(height: 12)
                            }
                        }

                        // Right margin label
                        Text(formatValue(boxModel.margin.right))
                            .font(.system(size: 9, design: .monospaced))
                            .foregroundColor(.orange)
                            .frame(width: 24)
                    }

                    // Bottom margin label
                    Text(formatValue(boxModel.margin.bottom))
                        .font(.system(size: 9, design: .monospaced))
                        .foregroundColor(.orange)
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
