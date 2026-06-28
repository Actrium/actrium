import SwiftUI
import Combine
import AppKit
import Highlightr

/// Source viewer panel for browsing and viewing source files
struct SourceViewerPanel: View {
    @StateObject private var viewModel = SourceViewerViewModel()
    @ObservedObject private var devTools = DevToolsManager.shared

    var body: some View {
        HSplitView {
            // File tree
            FileTreeView(
                nodes: viewModel.fileTree,
                selectedFile: $viewModel.selectedFile,
                isLoading: viewModel.isLoading,
                pathsToExpand: viewModel.pathsToExpand
            )
            .frame(minWidth: 180, maxWidth: 250)

            // Code viewer
            CodeViewerView(
                file: viewModel.selectedFile,
                content: viewModel.selectedContent,
                highlightLineNumber: viewModel.highlightLineNumber
            )
            .frame(minWidth: 300)
        }
        .task {
            await viewModel.loadSourceRoots()
            handlePendingNavigation()
        }
        .onAppear {
            handlePendingNavigation()
        }
        .onChange(of: devTools.pendingSourceNavigation) { _, _ in
            handlePendingNavigation()
        }
        .onChange(of: devTools.guestAppSourcePath) { _, _ in
            Task {
                await viewModel.loadSourceRoots()
            }
        }
    }

    private func handlePendingNavigation() {
        guard let nav = devTools.pendingSourceNavigation else { return }


        Task {
            if viewModel.fileTree.isEmpty {
                await viewModel.loadSourceRoots()
            }

            viewModel.navigateToSource(
                sourceFile: nav.fileName,
                sourceLine: nav.lineNumber
            )
            devTools.clearPendingSourceNavigation()
        }
    }
}

// MARK: - View Model

@MainActor
final class SourceViewerViewModel: ObservableObject {
    @Published var fileTree: [FileTreeNode] = []
    @Published var selectedFile: SourceFile? {
        didSet {
            if let file = selectedFile {
                // Load content on demand
                if file.content == nil {
                    selectedContent = try? String(contentsOfFile: file.fullPath, encoding: .utf8)
                } else {
                    selectedContent = file.content
                }
            } else {
                selectedContent = nil
            }
            // Clear highlight when manually selecting a file (not via navigation)
            if !isNavigating && highlightLineNumber != nil {
                highlightLineNumber = nil
            }
        }
    }
    @Published var selectedContent: String?
    @Published var isLoading = false
    @Published var error: String?
    @Published var highlightLineNumber: Int?
    /// Paths to expand in the file tree (set during navigation)
    @Published var pathsToExpand: Set<String> = []

    private var isNavigating = false

    /// Source file extensions to include
    private let sourceExtensions = Set(["ts", "tsx", "js", "jsx", "json", "css", "html", "md"])

    /// Load two source roots: Guest app and Host
    func loadSourceRoots() async {
        isLoading = true
        defer { isLoading = false }

        var roots: [FileTreeNode] = []

        // 1. Guest app root (from DevToolsManager)
        if let guestPath = DevToolsManager.shared.guestAppSourcePath {

            // Get the src directory if it exists, otherwise use the root
            let srcPath = (guestPath as NSString).appendingPathComponent("src")
            let scanPath = FileManager.default.fileExists(atPath: srcPath) ? srcPath : guestPath

            if let guestNode = scanDirectory(at: scanPath, displayName: "Guest", rootPath: guestPath) {
                roots.append(guestNode)
            }
        }

        // 2. Host root (fixed path)
        let hostPath = DevToolsManager.hostSourcePath
        if let hostNode = scanDirectory(at: hostPath, displayName: "Host", rootPath: hostPath) {
            roots.append(hostNode)
        }

        fileTree = roots
    }

    /// Scan a directory and build a tree node
    private func scanDirectory(at path: String, displayName: String, rootPath: String) -> FileTreeNode? {
        let fm = FileManager.default
        guard fm.fileExists(atPath: path) else { return nil }

        var children: [FileTreeNode] = []

        if let contents = try? fm.contentsOfDirectory(atPath: path) {
            for name in contents.sorted() {
                // Skip hidden files and node_modules
                if name.hasPrefix(".") || name == "node_modules" || name == "dist" || name == "build" {
                    continue
                }

                let fullPath = (path as NSString).appendingPathComponent(name)
                var isDir: ObjCBool = false
                fm.fileExists(atPath: fullPath, isDirectory: &isDir)

                if isDir.boolValue {
                    // Recursively scan subdirectory
                    if let subNode = scanDirectory(at: fullPath, displayName: name, rootPath: rootPath) {
                        children.append(subNode)
                    }
                } else {
                    // Check if it's a source file
                    let ext = (name as NSString).pathExtension.lowercased()
                    if sourceExtensions.contains(ext) {
                        let relativePath = makeRelativePath(fullPath, relativeTo: rootPath, rootName: displayName)
                        let file = SourceFile(fullPath: fullPath, content: nil)
                        children.append(FileTreeNode(
                            name: name,
                            path: relativePath,
                            isDirectory: false,
                            children: [],
                            file: file
                        ))
                    }
                }
            }
        }

        // Only return node if it has children (or is a root)
        if children.isEmpty && path != rootPath {
            return nil
        }

        let relativePath = makeRelativePath(path, relativeTo: rootPath, rootName: displayName)
        return FileTreeNode(
            name: displayName,
            path: relativePath,
            isDirectory: true,
            children: children,
            file: nil
        )
    }

    /// Make a relative path for display
    private func makeRelativePath(_ fullPath: String, relativeTo rootPath: String, rootName: String) -> String {
        if fullPath == rootPath {
            return rootName
        }
        let suffix = String(fullPath.dropFirst(rootPath.count + 1))
        return "\(rootName)/\(suffix)"
    }

    /// Navigate to source file by path
    func navigateToSource(sourceFile: String?, sourceLine: Int?) {
        guard let filePath = sourceFile, let line = sourceLine else {
            return
        }


        isNavigating = true
        defer { isNavigating = false }

        // Find the file in the tree and collect paths to expand
        var pathsToExpandList: [String] = []
        if let file = findFile(in: fileTree, fullPath: filePath, pathsToExpand: &pathsToExpandList) {
            pathsToExpand = Set(pathsToExpandList)
            selectedFile = file
            highlightLineNumber = line
            return
        }

        // Direct file system read as fallback
        if FileManager.default.fileExists(atPath: filePath) {
            let directFile = SourceFile(fullPath: filePath, content: try? String(contentsOfFile: filePath, encoding: .utf8))
            selectedFile = directFile
            highlightLineNumber = line
            return
        }

    }

    /// Find a file in the tree by full path, collecting directory paths along the way
    private func findFile(in nodes: [FileTreeNode], fullPath: String, pathsToExpand: inout [String]) -> SourceFile? {
        for node in nodes {
            if let file = node.file, file.fullPath == fullPath {
                return file
            }
            if node.isDirectory {
                // Check if the file might be in this directory
                if let found = findFile(in: node.children, fullPath: fullPath, pathsToExpand: &pathsToExpand) {
                    // Found in this subtree - add this directory's path
                    pathsToExpand.append(node.path)
                    return found
                }
            }
        }
        return nil
    }
}

// MARK: - File Tree View

struct FileTreeView: View {
    let nodes: [FileTreeNode]
    @Binding var selectedFile: SourceFile?
    let isLoading: Bool
    let pathsToExpand: Set<String>

    @State private var expandedPaths: Set<String> = []
    @State private var focusedPath: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Header
            HStack {
                Text("Sources")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundColor(.secondary)
                Spacer()
                if isLoading {
                    ProgressView()
                        .scaleEffect(0.5)
                }
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
            .background(Color(nsColor: .controlBackgroundColor))

            Divider()

            // Tree
            if nodes.isEmpty && !isLoading {
                VStack {
                    Spacer()
                    Text("No sources available")
                        .font(.system(size: 11))
                        .foregroundColor(.secondary)
                    Text("Make sure Metro is running")
                        .font(.system(size: 10))
                        .foregroundColor(.secondary.opacity(0.7))
                    Spacer()
                }
                .frame(maxWidth: .infinity)
            } else {
                ScrollViewReader { proxy in
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 0) {
                            ForEach(flattenedNodes) { item in
                                FileTreeRowView(
                                    node: item.node,
                                    depth: item.depth,
                                    isExpanded: expandedPaths.contains(item.node.path),
                                    isSelected: selectedFile != nil && selectedFile?.fullPath == item.node.file?.fullPath,
                                    isFocused: focusedPath == item.node.path,
                                    onTap: { handleTap(item.node) },
                                    onToggle: { toggleExpanded(item.node.path) }
                                )
                                .id(item.node.path)
                            }
                        }
                        .padding(.vertical, 4)
                    }
                    .focusable()
                    .focusEffectDisabled()
                    .onKeyPress(.upArrow) { navigateUp(); return .handled }
                    .onKeyPress(.downArrow) { navigateDown(); return .handled }
                    .onKeyPress(.leftArrow) { navigateLeft(); return .handled }
                    .onKeyPress(.rightArrow) { navigateRight(); return .handled }
                    .onKeyPress(.return) { handleReturn(); return .handled }
                    .onChange(of: focusedPath) { _, newPath in
                        if let path = newPath {
                            withAnimation(.easeInOut(duration: 0.15)) {
                                proxy.scrollTo(path, anchor: .center)
                            }
                        }
                    }
                    .onChange(of: selectedFile?.fullPath) { _, newPath in
                        // Scroll to selected file when it changes via navigation
                        if let path = newPath,
                           let node = findNodeByFullPath(path) {
                            DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) {
                                withAnimation(.easeInOut(duration: 0.2)) {
                                    proxy.scrollTo(node.path, anchor: .center)
                                }
                            }
                        }
                    }
                }
            }
        }
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.5))
        .onAppear {
            // Auto-expand first level
            for node in nodes where node.isDirectory {
                expandedPaths.insert(node.path)
            }
        }
        .onChange(of: pathsToExpand) { _, newPaths in
            // Expand directories when navigation happens
            if !newPaths.isEmpty {
                expandedPaths.formUnion(newPaths)
            }
        }
    }

    /// Find a tree node by its file's full path
    private func findNodeByFullPath(_ fullPath: String) -> FileTreeNode? {
        func find(in nodes: [FileTreeNode]) -> FileTreeNode? {
            for node in nodes {
                if node.file?.fullPath == fullPath {
                    return node
                }
                if let found = find(in: node.children) {
                    return found
                }
            }
            return nil
        }
        return find(in: nodes)
    }

    // Flattened visible nodes for navigation
    private var flattenedNodes: [FlattenedNode] {
        var result: [FlattenedNode] = []
        func flatten(_ nodes: [FileTreeNode], depth: Int) {
            for node in nodes {
                result.append(FlattenedNode(node: node, depth: depth))
                if node.isDirectory && expandedPaths.contains(node.path) {
                    flatten(node.children, depth: depth + 1)
                }
            }
        }
        flatten(nodes, depth: 0)
        return result
    }

    private func handleTap(_ node: FileTreeNode) {
        focusedPath = node.path
        if let file = node.file {
            selectedFile = file
        } else if node.isDirectory {
            toggleExpanded(node.path)
        }
    }

    private func toggleExpanded(_ path: String) {
        if expandedPaths.contains(path) {
            expandedPaths.remove(path)
        } else {
            expandedPaths.insert(path)
        }
    }

    private func navigateUp() {
        let items = flattenedNodes
        guard !items.isEmpty else { return }

        if let current = focusedPath,
           let idx = items.firstIndex(where: { $0.node.path == current }),
           idx > 0 {
            focusedPath = items[idx - 1].node.path
        } else {
            focusedPath = items.first?.node.path
        }
    }

    private func navigateDown() {
        let items = flattenedNodes
        guard !items.isEmpty else { return }

        if let current = focusedPath,
           let idx = items.firstIndex(where: { $0.node.path == current }),
           idx < items.count - 1 {
            focusedPath = items[idx + 1].node.path
        } else {
            focusedPath = items.first?.node.path
        }
    }

    private func navigateLeft() {
        guard let current = focusedPath,
              let item = flattenedNodes.first(where: { $0.node.path == current }) else { return }

        if item.node.isDirectory && expandedPaths.contains(current) {
            // Collapse current directory
            expandedPaths.remove(current)
        } else {
            // Go to parent directory
            let components = current.components(separatedBy: "/")
            if components.count > 1 {
                let parentPath = components.dropLast().joined(separator: "/")
                focusedPath = parentPath
            }
        }
    }

    private func navigateRight() {
        guard let current = focusedPath,
              let item = flattenedNodes.first(where: { $0.node.path == current }) else { return }

        if item.node.isDirectory {
            if !expandedPaths.contains(current) {
                // Expand directory
                expandedPaths.insert(current)
            } else if !item.node.children.isEmpty {
                // Move to first child
                focusedPath = item.node.children.first?.path
            }
        }
    }

    private func handleReturn() {
        guard let current = focusedPath,
              let item = flattenedNodes.first(where: { $0.node.path == current }) else { return }

        if let file = item.node.file {
            selectedFile = file
        } else if item.node.isDirectory {
            toggleExpanded(current)
        }
    }
}

private struct FlattenedNode: Identifiable {
    let node: FileTreeNode
    let depth: Int
    var id: String { node.path }
}

// MARK: - File Tree Row View (non-recursive, state managed by parent)

struct FileTreeRowView: View {
    let node: FileTreeNode
    let depth: Int
    let isExpanded: Bool
    let isSelected: Bool
    let isFocused: Bool
    let onTap: () -> Void
    let onToggle: () -> Void

    var body: some View {
        HStack(spacing: 4) {
            // Indentation
            if depth > 0 {
                Spacer()
                    .frame(width: CGFloat(depth) * 12)
            }

            // Expand/collapse for directories
            if node.isDirectory {
                Image(systemName: isExpanded ? "chevron.down" : "chevron.right")
                    .font(.system(size: 8, weight: .semibold))
                    .foregroundColor(.secondary)
                    .frame(width: 12)
                    .onTapGesture { onToggle() }
            } else {
                Spacer().frame(width: 12)
            }

            // Icon
            Image(systemName: node.isDirectory ? "folder.fill" : fileIcon(for: node.name))
                .font(.system(size: 11))
                .foregroundColor(node.isDirectory ? .orange : iconColor(for: node.name))

            // Name
            Text(node.name)
                .font(.system(size: 11))
                .foregroundColor(isSelected ? .white : .primary)
                .lineLimit(1)

            Spacer()
        }
        .padding(.horizontal, 6)
        .padding(.vertical, 3)
        .background(
            RoundedRectangle(cornerRadius: 4)
                .fill(backgroundColor)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 4)
                .stroke(isFocused && !isSelected ? Color.accentColor.opacity(0.5) : Color.clear, lineWidth: 1)
        )
        .contentShape(Rectangle())
        .onTapGesture { onTap() }
    }

    private var backgroundColor: Color {
        if isSelected {
            return Color.accentColor
        } else if isFocused {
            return Color.accentColor.opacity(0.15)
        }
        return Color.clear
    }

    private func fileIcon(for name: String) -> String {
        let ext = (name as NSString).pathExtension.lowercased()
        switch ext {
        case "tsx", "jsx": return "doc.text.fill"
        case "ts", "js": return "doc.fill"
        case "json": return "curlybraces"
        case "css": return "paintbrush.fill"
        case "png", "jpg", "jpeg", "gif", "webp", "ico": return "photo.fill"
        case "svg": return "square.on.circle"
        case "ttf", "otf", "woff", "woff2": return "textformat"
        case "mp3", "wav": return "waveform"
        case "mp4", "mov": return "video.fill"
        default: return "doc.fill"
        }
    }

    private func iconColor(for name: String) -> Color {
        let ext = (name as NSString).pathExtension.lowercased()
        switch ext {
        case "tsx", "jsx": return .cyan
        case "ts": return .blue
        case "js": return .yellow
        case "json": return .orange
        case "css": return .pink
        case "png", "jpg", "jpeg", "gif", "webp", "ico", "svg": return .green
        case "ttf", "otf", "woff", "woff2": return .purple
        case "mp3", "wav": return .red
        case "mp4", "mov": return .indigo
        default: return .secondary
        }
    }
}

// MARK: - Code Viewer View

struct CodeViewerView: View {
    let file: SourceFile?
    let content: String?
    var highlightLineNumber: Int? = nil

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Header
            HStack {
                if let file = file {
                    Image(systemName: fileIcon(for: file))
                        .font(.system(size: 10))
                        .foregroundColor(.secondary)
                    Text(file.relativePath)
                        .font(.system(size: 11, weight: .medium))
                        .foregroundColor(.primary)
                    if let line = highlightLineNumber {
                        Text(":\(line)")
                            .font(.system(size: 11, weight: .medium))
                            .foregroundColor(.orange)
                    }
                } else {
                    Text("Select a file to view")
                        .font(.system(size: 11))
                        .foregroundColor(.secondary)
                }
                Spacer()
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .background(Color(nsColor: .controlBackgroundColor))

            Divider()

            // Content
            if let file = file {
                if file.isImage {
                    // Image preview
                    ImagePreviewView(file: file)
                } else if let content = content {
                    // Code view
                    NativeCodeView(code: content, language: file.language, scrollToLine: highlightLineNumber)
                } else if file.isAsset {
                    // Asset without preview
                    AssetInfoView(file: file)
                } else {
                    // Loading or no content
                    VStack {
                        Spacer()
                        ProgressView()
                        Text("Loading...")
                            .font(.system(size: 11))
                            .foregroundColor(.secondary)
                        Spacer()
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(Color(nsColor: .textBackgroundColor))
                }
            } else {
                VStack {
                    Spacer()
                    Image(systemName: "doc.text.magnifyingglass")
                        .font(.system(size: 32))
                        .foregroundColor(.secondary.opacity(0.5))
                    Text("No file selected")
                        .font(.system(size: 12))
                        .foregroundColor(.secondary)
                    Spacer()
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Color(nsColor: .textBackgroundColor))
            }
        }
    }

    private func fileIcon(for file: SourceFile) -> String {
        if file.isImage { return "photo" }
        if file.isFont { return "textformat" }
        return "doc.text"
    }
}

// MARK: - Image Preview View

struct ImagePreviewView: View {
    let file: SourceFile

    var body: some View {
        ScrollView([.horizontal, .vertical]) {
            VStack(spacing: 16) {
                if let image = NSImage(contentsOfFile: file.fullPath) {
                    Image(nsImage: image)
                        .resizable()
                        .aspectRatio(contentMode: .fit)
                        .frame(maxWidth: 600, maxHeight: 400)
                        .background(
                            // Checkerboard pattern for transparency
                            CheckerboardPattern()
                        )
                        .cornerRadius(4)
                        .shadow(radius: 2)

                    // Image info
                    VStack(spacing: 4) {
                        Text("\(Int(image.size.width)) × \(Int(image.size.height))")
                            .font(.system(size: 12, weight: .medium, design: .monospaced))
                            .foregroundColor(.primary)

                        if let fileSize = try? FileManager.default.attributesOfItem(atPath: file.fullPath)[.size] as? Int {
                            Text(formatFileSize(fileSize))
                                .font(.system(size: 11))
                                .foregroundColor(.secondary)
                        }
                    }
                } else {
                    Image(systemName: "photo")
                        .font(.system(size: 48))
                        .foregroundColor(.secondary)
                    Text("Unable to load image")
                        .font(.system(size: 12))
                        .foregroundColor(.secondary)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .padding(20)
        }
        .background(Color(red: 0.12, green: 0.12, blue: 0.14))
    }

    private func formatFileSize(_ bytes: Int) -> String {
        if bytes < 1024 { return "\(bytes) B" }
        if bytes < 1024 * 1024 { return String(format: "%.1f KB", Double(bytes) / 1024) }
        return String(format: "%.1f MB", Double(bytes) / (1024 * 1024))
    }
}

// MARK: - Checkerboard Pattern (for transparency)

struct CheckerboardPattern: View {
    let size: CGFloat = 10

    var body: some View {
        Canvas { context, canvasSize in
            let rows = Int(canvasSize.height / size) + 1
            let cols = Int(canvasSize.width / size) + 1
            for row in 0..<rows {
                for col in 0..<cols {
                    let isLight = (row + col) % 2 == 0
                    let rect = CGRect(x: CGFloat(col) * size, y: CGFloat(row) * size, width: size, height: size)
                    context.fill(Path(rect), with: .color(isLight ? Color.white.opacity(0.3) : Color.black.opacity(0.1)))
                }
            }
        }
    }
}

// MARK: - Asset Info View (for non-previewable assets)

struct AssetInfoView: View {
    let file: SourceFile

    var body: some View {
        VStack(spacing: 12) {
            Spacer()
            Image(systemName: iconForFile(file))
                .font(.system(size: 48))
                .foregroundColor(.secondary)

            Text(file.name)
                .font(.system(size: 14, weight: .medium))
                .foregroundColor(.primary)

            if let fileSize = try? FileManager.default.attributesOfItem(atPath: file.fullPath)[.size] as? Int {
                Text(formatFileSize(fileSize))
                    .font(.system(size: 12))
                    .foregroundColor(.secondary)
            }

            Text(file.fileExtension.uppercased())
                .font(.system(size: 10, weight: .semibold))
                .foregroundColor(.white)
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .background(Color.blue.opacity(0.8))
                .cornerRadius(4)

            Spacer()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(nsColor: .textBackgroundColor))
    }

    private func iconForFile(_ file: SourceFile) -> String {
        if file.isFont { return "textformat" }
        let ext = file.fileExtension.lowercased()
        switch ext {
        case "mp3", "wav": return "waveform"
        case "mp4", "mov": return "video"
        case "json": return "curlybraces"
        default: return "doc"
        }
    }

    private func formatFileSize(_ bytes: Int) -> String {
        if bytes < 1024 { return "\(bytes) B" }
        if bytes < 1024 * 1024 { return String(format: "%.1f KB", Double(bytes) / 1024) }
        return String(format: "%.1f MB", Double(bytes) / (1024 * 1024))
    }
}

// MARK: - Native Code View (NSTextView-based for performance)

struct NativeCodeView: NSViewRepresentable {
    let code: String
    let language: String
    var scrollToLine: Int? = nil

    // Code font - 11px monospace to match Chrome DevTools
    private static let codeFont = NSFont.monospacedSystemFont(ofSize: 11, weight: .regular)
    private static let lineNumberFont = NSFont.monospacedSystemFont(ofSize: 11, weight: .regular)

    // Dark theme colors
    private static let backgroundColor = NSColor(red: 0.1, green: 0.1, blue: 0.12, alpha: 1.0)
    private static let textColor = NSColor(red: 0.67, green: 0.76, blue: 0.69, alpha: 1.0) // Light green
    private static let lineNumberColor = NSColor.secondaryLabelColor.withAlphaComponent(0.5)

    // Highlightr instance
    private static let highlightr: Highlightr? = {
        let h = Highlightr()
        h?.setTheme(to: "atom-one-dark")
        h?.theme.codeFont = codeFont
        return h
    }()

    // Cache
    private static var highlightCache: [String: NSAttributedString] = [:]
    private static let cacheLock = NSLock()

    func makeNSView(context: Context) -> NSScrollView {
        let scrollView = NSScrollView()
        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.borderType = .noBorder
        scrollView.drawsBackground = true
        scrollView.backgroundColor = Self.backgroundColor

        let textView = LineNumberTextView()
        textView.isEditable = false
        textView.isSelectable = true
        textView.drawsBackground = true
        textView.backgroundColor = Self.backgroundColor
        textView.textColor = Self.textColor
        textView.font = Self.codeFont
        textView.isRichText = true
        textView.allowsUndo = false
        textView.usesFindPanel = true
        textView.isAutomaticQuoteSubstitutionEnabled = false
        textView.isAutomaticDashSubstitutionEnabled = false
        textView.isAutomaticTextReplacementEnabled = false
        textView.isAutomaticSpellingCorrectionEnabled = false

        // Configure text container for horizontal scrolling
        textView.textContainer?.containerSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        textView.textContainer?.widthTracksTextView = false
        textView.isHorizontallyResizable = true
        textView.isVerticallyResizable = true
        textView.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)

        // Line number styling
        textView.lineNumberFont = Self.lineNumberFont
        textView.lineNumberColor = Self.lineNumberColor

        scrollView.documentView = textView

        return scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        guard let textView = scrollView.documentView as? LineNumberTextView else { return }

        // Check if content changed
        let currentCode = textView.string
        let contentChanged = currentCode != code

        if contentChanged {
            // Set plain text immediately for responsiveness
            let plainAttr = NSAttributedString(string: code, attributes: [
                .font: Self.codeFont,
                .foregroundColor: Self.textColor
            ])
            textView.textStorage?.setAttributedString(plainAttr)

            // Highlight async (runs in background, NSTextView can handle large attributed strings)
            highlightAsync(code: code, language: language, textView: textView)
        }

        // Update highlighted line
        textView.highlightedLine = scrollToLine

        // Scroll to line if specified
        if let lineNumber = scrollToLine, contentChanged || textView.lastScrolledLine != lineNumber {
            textView.lastScrolledLine = lineNumber
            DispatchQueue.main.async {
                self.scrollToLine(lineNumber, in: textView, scrollView: scrollView)
            }
        }
    }

    private func scrollToLine(_ lineNumber: Int, in textView: NSTextView, scrollView: NSScrollView) {
        let text = textView.string as NSString
        var currentLine = 1
        var lineStart = 0

        // Find the character index of the target line
        for i in 0..<text.length {
            if currentLine == lineNumber {
                lineStart = i
                break
            }
            if text.character(at: i) == UInt16(UnicodeScalar("\n").value) {
                currentLine += 1
            }
        }

        // Get the rect for that character position
        let layoutManager = textView.layoutManager!
        let glyphRange = layoutManager.glyphRange(forCharacterRange: NSRange(location: lineStart, length: 0), actualCharacterRange: nil)
        let lineRect = layoutManager.boundingRect(forGlyphRange: glyphRange, in: textView.textContainer!)

        // Scroll to make that line visible (with some padding)
        let targetY = max(0, lineRect.origin.y - 100) // 100px padding from top
        scrollView.contentView.scroll(to: NSPoint(x: 0, y: targetY))
        scrollView.reflectScrolledClipView(scrollView.contentView)

        // Force redraw to show highlight
        textView.needsDisplay = true
    }

    private func highlightAsync(code: String, language: String, textView: NSTextView) {
        let cacheKey = "\(language):\(code.hashValue)"

        // Check cache
        Self.cacheLock.lock()
        if let cached = Self.highlightCache[cacheKey] {
            Self.cacheLock.unlock()
            textView.textStorage?.setAttributedString(cached)
            return
        }
        Self.cacheLock.unlock()

        // Highlight on background thread
        Task.detached(priority: .userInitiated) {
            guard let highlightr = Self.highlightr,
                  let highlighted = highlightr.highlight(code, as: language) else {
                return
            }

            // Apply uniform font
            let mutableAttr = NSMutableAttributedString(attributedString: highlighted)
            mutableAttr.addAttribute(.font, value: Self.codeFont, range: NSRange(location: 0, length: mutableAttr.length))

            // Cache
            Self.cacheLock.lock()
            if Self.highlightCache.count > 30 {
                Self.highlightCache.removeAll()
            }
            Self.highlightCache[cacheKey] = mutableAttr
            Self.cacheLock.unlock()

            await MainActor.run {
                textView.textStorage?.setAttributedString(mutableAttr)
            }
        }
    }
}

// MARK: - Line Number Text View

class LineNumberTextView: NSTextView {
    var lineNumberFont: NSFont = NSFont.monospacedSystemFont(ofSize: 11, weight: .regular)
    var lineNumberColor: NSColor = .secondaryLabelColor
    var highlightedLine: Int? = nil
    var lastScrolledLine: Int? = nil

    private let lineNumberWidth: CGFloat = 50
    private let lineNumberPadding: CGFloat = 8
    private let highlightColor = NSColor.orange.withAlphaComponent(0.2)
    private var scrollObserver: NSObjectProtocol?

    override var textContainerOrigin: NSPoint {
        return NSPoint(x: lineNumberWidth + lineNumberPadding, y: 10)
    }

    override func viewDidMoveToSuperview() {
        super.viewDidMoveToSuperview()
        setupScrollObserver()
    }

    private func setupScrollObserver() {
        // Remove existing observer
        if let observer = scrollObserver {
            NotificationCenter.default.removeObserver(observer)
        }

        // Observe scroll changes to redraw line numbers
        if let scrollView = enclosingScrollView {
            scrollObserver = NotificationCenter.default.addObserver(
                forName: NSView.boundsDidChangeNotification,
                object: scrollView.contentView,
                queue: .main
            ) { [weak self] _ in
                self?.needsDisplay = true
            }
            scrollView.contentView.postsBoundsChangedNotifications = true
        }
    }

    deinit {
        if let observer = scrollObserver {
            NotificationCenter.default.removeObserver(observer)
        }
    }

    override func draw(_ dirtyRect: NSRect) {
        // Draw gutter background first (before clipping)
        drawGutterBackground()

        // Draw line highlight if set
        drawLineHighlight()

        super.draw(dirtyRect)

        // Draw line numbers and separator on top (after super.draw to avoid being covered)
        drawLineNumbers()
        drawSeparatorLine()
    }

    private func drawLineHighlight() {
        guard let lineNumber = highlightedLine,
              let layoutManager = layoutManager,
              let textContainer = textContainer else { return }

        let content = string as NSString
        guard content.length > 0 else { return }

        // Find the character range for the highlighted line
        var currentLine = 1
        var lineStart = 0
        var lineEnd = 0

        for i in 0..<content.length {
            if currentLine == lineNumber {
                lineStart = i
                // Find end of line
                for j in i..<content.length {
                    if content.character(at: j) == UInt16(UnicodeScalar("\n").value) {
                        lineEnd = j
                        break
                    }
                    lineEnd = j + 1
                }
                break
            }
            if content.character(at: i) == UInt16(UnicodeScalar("\n").value) {
                currentLine += 1
            }
        }

        if currentLine != lineNumber { return }

        // Get the rect for that line
        let charRange = NSRange(location: lineStart, length: max(1, lineEnd - lineStart))
        let glyphRange = layoutManager.glyphRange(forCharacterRange: charRange, actualCharacterRange: nil)
        let lineRect = layoutManager.boundingRect(forGlyphRange: glyphRange, in: textContainer)

        // Draw highlight across the full width
        NSGraphicsContext.saveGraphicsState()
        NSBezierPath(rect: bounds).setClip()

        let highlightRect = NSRect(
            x: 0,
            y: lineRect.origin.y + textContainerOrigin.y,
            width: max(bounds.width, 2000),
            height: lineRect.height
        )
        highlightColor.setFill()
        highlightRect.fill()

        NSGraphicsContext.restoreGraphicsState()
    }

    private func drawGutterBackground() {
        guard let scrollView = enclosingScrollView else { return }
        let visibleRect = scrollView.documentVisibleRect

        NSGraphicsContext.saveGraphicsState()
        // Reset clipping to draw in gutter area
        NSBezierPath(rect: bounds).setClip()

        // Gutter background - slightly darker than code background
        let gutterRect = NSRect(x: 0, y: visibleRect.origin.y, width: lineNumberWidth, height: visibleRect.height + 100)
        NSColor(red: 0.08, green: 0.08, blue: 0.1, alpha: 1.0).setFill()
        gutterRect.fill()

        NSGraphicsContext.restoreGraphicsState()
    }

    private func drawSeparatorLine() {
        guard let scrollView = enclosingScrollView else { return }
        let visibleRect = scrollView.documentVisibleRect

        NSGraphicsContext.saveGraphicsState()
        NSBezierPath(rect: bounds).setClip()

        // Separator line - at the right edge of gutter, bright enough to be visible
        let separatorX = lineNumberWidth + 1
        let separatorRect = NSRect(x: separatorX, y: visibleRect.origin.y, width: 1, height: visibleRect.height + 100)
        NSColor(red: 0.4, green: 0.4, blue: 0.45, alpha: 1.0).setFill()
        separatorRect.fill()

        NSGraphicsContext.restoreGraphicsState()
    }

    private func drawLineNumbers() {
        guard let layoutManager = layoutManager,
              let textContainer = textContainer,
              let scrollView = enclosingScrollView else { return }

        let visibleRect = scrollView.documentVisibleRect
        let content = string as NSString

        guard content.length > 0 else { return }

        NSGraphicsContext.saveGraphicsState()
        // Reset clipping to draw in gutter area
        NSBezierPath(rect: bounds).setClip()

        // Draw line numbers
        let glyphRange = layoutManager.glyphRange(forBoundingRect: visibleRect, in: textContainer)
        guard glyphRange.length > 0 else {
            NSGraphicsContext.restoreGraphicsState()
            return
        }

        var lineNumber = 1

        // Count lines before visible range
        let charRange = layoutManager.characterRange(forGlyphRange: glyphRange, actualGlyphRange: nil)
        if charRange.location > 0 {
            let beforeText = content.substring(to: charRange.location)
            lineNumber = beforeText.components(separatedBy: "\n").count
        }

        let attributes: [NSAttributedString.Key: Any] = [
            .font: lineNumberFont,
            .foregroundColor: lineNumberColor
        ]

        // Highlighted line attributes (brighter)
        let highlightedAttributes: [NSAttributedString.Key: Any] = [
            .font: lineNumberFont,
            .foregroundColor: NSColor.orange
        ]

        // Arrow marker for navigation target
        let arrowAttributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 10, weight: .bold),
            .foregroundColor: NSColor.orange
        ]

        layoutManager.enumerateLineFragments(forGlyphRange: glyphRange) { [weak self] (rect, usedRect, container, range, stop) in
            guard let self = self else { return }

            let isHighlighted = lineNumber == self.highlightedLine
            let currentAttrs = isHighlighted ? highlightedAttributes : attributes

            let lineStr = "\(lineNumber)" as NSString
            let size = lineStr.size(withAttributes: currentAttrs)
            let y = rect.origin.y + self.textContainerOrigin.y + (rect.height - size.height) / 2
            let x = self.lineNumberWidth - size.width - 4

            // Draw arrow marker for highlighted line
            if isHighlighted {
                let arrow = "▶" as NSString
                let arrowSize = arrow.size(withAttributes: arrowAttributes)
                arrow.draw(at: NSPoint(x: 4, y: y + (size.height - arrowSize.height) / 2), withAttributes: arrowAttributes)
            }

            lineStr.draw(at: NSPoint(x: x, y: y), withAttributes: currentAttrs)

            // Check if this line fragment ends with newline
            let charRange = layoutManager.characterRange(forGlyphRange: range, actualGlyphRange: nil)
            if charRange.location + charRange.length <= content.length {
                let endIndex = min(charRange.location + charRange.length, content.length)
                if endIndex > 0 && content.character(at: endIndex - 1) == 10 { // newline
                    lineNumber += 1
                }
            }
        }

        NSGraphicsContext.restoreGraphicsState()
    }

    override func setNeedsDisplay(_ invalidRect: NSRect) {
        super.setNeedsDisplay(bounds)
    }
}

// MARK: - Preview

#Preview("Source Viewer") {
    SourceViewerPanel()
        .frame(width: 800, height: 500)
}
