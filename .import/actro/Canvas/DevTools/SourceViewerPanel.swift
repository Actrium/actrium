import SwiftUI
import Combine
import Highlightr

/// Source viewer panel for browsing and viewing source files
struct SourceViewerPanel: View {
    @StateObject private var viewModel = SourceViewerViewModel()

    var body: some View {
        HSplitView {
            // File tree
            FileTreeView(
                nodes: viewModel.fileTree,
                selectedFile: $viewModel.selectedFile,
                isLoading: viewModel.isLoading
            )
            .frame(minWidth: 180, maxWidth: 250)

            // Code viewer
            CodeViewerView(
                file: viewModel.selectedFile,
                content: viewModel.selectedContent
            )
            .frame(minWidth: 300)
        }
        .task {
            await viewModel.loadSourceMap()
        }
    }
}

// MARK: - View Model

@MainActor
final class SourceViewerViewModel: ObservableObject {
    @Published var files: [SourceFile] = []
    @Published var fileTree: [FileTreeNode] = []
    @Published var selectedFile: SourceFile?
    @Published var selectedContent: String?
    @Published var isLoading = false
    @Published var error: String?

    func loadSourceMap() async {
        isLoading = true
        defer { isLoading = false }

        do {
            let projectFiles = try await MetroService.shared.getProjectFiles()
            files = projectFiles
            fileTree = FileTreeNode.buildTree(from: projectFiles)
        } catch {
            self.error = error.localizedDescription
        }
    }

    func selectFile(_ file: SourceFile) {
        selectedFile = file
        selectedContent = file.content
    }
}

// MARK: - File Tree View

struct FileTreeView: View {
    let nodes: [FileTreeNode]
    @Binding var selectedFile: SourceFile?
    let isLoading: Bool

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
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        ForEach(nodes) { node in
                            FileTreeNodeView(
                                node: node,
                                selectedFile: $selectedFile,
                                depth: 0
                            )
                        }
                    }
                    .padding(.vertical, 4)
                }
            }
        }
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.5))
    }
}

// MARK: - File Tree Node View

struct FileTreeNodeView: View {
    let node: FileTreeNode
    @Binding var selectedFile: SourceFile?
    let depth: Int
    @State private var isExpanded = true

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Node row
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
                        .onTapGesture {
                            withAnimation(.easeInOut(duration: 0.15)) {
                                isExpanded.toggle()
                            }
                        }
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
                    .foregroundColor(selectedFile?.fullPath == node.file?.fullPath ? .white : .primary)
                    .lineLimit(1)

                Spacer()
            }
            .padding(.horizontal, 6)
            .padding(.vertical, 3)
            .background(
                RoundedRectangle(cornerRadius: 4)
                    .fill(selectedFile?.fullPath == node.file?.fullPath ? Color.accentColor : Color.clear)
            )
            .contentShape(Rectangle())
            .onTapGesture {
                if let file = node.file {
                    selectedFile = file
                } else if node.isDirectory {
                    withAnimation(.easeInOut(duration: 0.15)) {
                        isExpanded.toggle()
                    }
                }
            }

            // Children
            if node.isDirectory && isExpanded {
                ForEach(node.children.sorted { sortNodes($0, $1) }) { child in
                    FileTreeNodeView(
                        node: child,
                        selectedFile: $selectedFile,
                        depth: depth + 1
                    )
                }
            }
        }
    }

    private func sortNodes(_ a: FileTreeNode, _ b: FileTreeNode) -> Bool {
        if a.isDirectory != b.isDirectory {
            return a.isDirectory
        }
        return a.name.lowercased() < b.name.lowercased()
    }

    private func fileIcon(for name: String) -> String {
        let ext = (name as NSString).pathExtension.lowercased()
        switch ext {
        case "tsx", "jsx": return "doc.text.fill"
        case "ts", "js": return "doc.fill"
        case "json": return "curlybraces"
        case "css": return "paintbrush.fill"
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
        default: return .secondary
        }
    }
}

// MARK: - Code Viewer View

struct CodeViewerView: View {
    let file: SourceFile?
    let content: String?

    private static let highlightr: Highlightr? = {
        let h = Highlightr()
        h?.setTheme(to: "atom-one-dark")
        return h
    }()

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Header
            HStack {
                if let file = file {
                    Image(systemName: "doc.text")
                        .font(.system(size: 10))
                        .foregroundColor(.secondary)
                    Text(file.relativePath)
                        .font(.system(size: 11, weight: .medium))
                        .foregroundColor(.primary)
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
            if let content = content, let file = file {
                ScrollView([.horizontal, .vertical]) {
                    highlightedCode(content, language: file.language)
                        .padding(10)
                }
                .background(Color(red: 0.1, green: 0.1, blue: 0.12))
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

    @ViewBuilder
    private func highlightedCode(_ code: String, language: String) -> some View {
        let lines = code.components(separatedBy: "\n")
        let lineNumberWidth = CGFloat(String(lines.count).count) * 8 + 16

        HStack(alignment: .top, spacing: 0) {
            // Line numbers
            VStack(alignment: .trailing, spacing: 0) {
                ForEach(Array(lines.enumerated()), id: \.offset) { index, _ in
                    Text("\(index + 1)")
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundColor(.secondary.opacity(0.5))
                        .frame(height: 16)
                }
            }
            .frame(width: lineNumberWidth)
            .padding(.trailing, 8)

            Divider()
                .background(Color.secondary.opacity(0.2))

            // Code
            VStack(alignment: .leading, spacing: 0) {
                if let highlightr = Self.highlightr,
                   let highlighted = highlightr.highlight(code, as: language) {
                    Text(AttributedString(highlighted))
                        .font(.system(size: 11, design: .monospaced))
                        .textSelection(.enabled)
                } else {
                    Text(code)
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundColor(.green.opacity(0.9))
                        .textSelection(.enabled)
                }
            }
            .padding(.leading, 8)
        }
    }
}

// MARK: - Preview

#Preview("Source Viewer") {
    SourceViewerPanel()
        .frame(width: 800, height: 500)
}
