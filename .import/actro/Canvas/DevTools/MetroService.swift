import Foundation

/// Service for interacting with Metro bundler
actor MetroService {
    static let shared = MetroService()

    private var cachedSourceMap: SourceMap?
    private var lastFetchTime: Date?
    private let cacheTimeout: TimeInterval = 30 // Refresh every 30 seconds

    private init() {}

    /// Fetch source map from Metro
    func fetchSourceMap(port: Int = 8082) async throws -> SourceMap {
        // Return cached if still valid
        if let cached = cachedSourceMap,
           let lastFetch = lastFetchTime,
           Date().timeIntervalSince(lastFetch) < cacheTimeout {
            return cached
        }

        let url = URL(string: "http://localhost:\(port)/index.map?platform=macos&dev=true")!
        let (data, _) = try await URLSession.shared.data(from: url)
        let sourceMap = try JSONDecoder().decode(SourceMap.self, from: data)

        cachedSourceMap = sourceMap
        lastFetchTime = Date()

        return sourceMap
    }

    /// Get list of project source files (excluding node_modules)
    func getProjectFiles(port: Int = 8082) async throws -> [SourceFile] {
        let sourceMap = try await fetchSourceMap(port: port)
        return sourceMap.projectFiles
    }

    /// Get source content for a specific file
    func getSourceContent(for path: String, port: Int = 8082) async throws -> String? {
        let sourceMap = try await fetchSourceMap(port: port)
        return sourceMap.getContent(for: path)
    }

    /// Invalidate cache
    func invalidateCache() {
        cachedSourceMap = nil
        lastFetchTime = nil
    }
}

// MARK: - Source Map Model

struct SourceMap: Decodable, Sendable {
    let version: Int
    let sources: [String]
    let sourcesContent: [String?]?
    let names: [String]?
    let mappings: String

    /// Get project files (excluding node_modules)
    var projectFiles: [SourceFile] {
        var files: [SourceFile] = []
        for (index, path) in sources.enumerated() {
            // Skip internal/polyfill files
            guard !path.starts(with: "__") else { continue }
            // Skip node_modules
            guard !path.contains("/node_modules/") else { continue }

            let content = sourcesContent?[safe: index] ?? nil
            files.append(SourceFile(
                fullPath: path,
                content: content
            ))
        }
        return files
    }

    /// Get content for a specific file path
    func getContent(for path: String) -> String? {
        guard let index = sources.firstIndex(of: path),
              let contents = sourcesContent,
              index < contents.count else {
            return nil
        }
        return contents[index]
    }
}

/// Represents a source file from the bundle
struct SourceFile: Identifiable, Hashable, Sendable {
    let id = UUID()
    let fullPath: String
    let content: String?

    /// File name only
    var name: String {
        (fullPath as NSString).lastPathComponent
    }

    /// Directory path
    var directory: String {
        (fullPath as NSString).deletingLastPathComponent
    }

    /// Relative path from project root
    var relativePath: String {
        // Find common project roots and make relative
        if let range = fullPath.range(of: "/Subsystem/") {
            return String(fullPath[range.upperBound...])
        }
        if let range = fullPath.range(of: "/rill/") {
            return "rill/" + String(fullPath[range.upperBound...])
        }
        if let range = fullPath.range(of: "/askit/") {
            return "askit/" + String(fullPath[range.upperBound...])
        }
        return name
    }

    /// File extension
    var fileExtension: String {
        (fullPath as NSString).pathExtension
    }

    /// Language for syntax highlighting
    var language: String {
        switch fileExtension.lowercased() {
        case "ts", "tsx": return "typescript"
        case "js", "jsx": return "javascript"
        case "json": return "json"
        case "css": return "css"
        default: return "javascript"
        }
    }

    func hash(into hasher: inout Hasher) {
        hasher.combine(fullPath)
    }

    static func == (lhs: SourceFile, rhs: SourceFile) -> Bool {
        lhs.fullPath == rhs.fullPath
    }
}

// MARK: - File Tree Node

struct FileTreeNode: Identifiable {
    let id = UUID()
    let name: String
    let path: String
    let isDirectory: Bool
    var children: [FileTreeNode]
    var file: SourceFile?

    static func buildTree(from files: [SourceFile]) -> [FileTreeNode] {
        // Use a class-based builder for proper reference semantics
        class TreeBuilder {
            var name: String
            var path: String
            var isDirectory: Bool
            var children: [String: TreeBuilder] = [:]
            var file: SourceFile?

            init(name: String, path: String, isDirectory: Bool, file: SourceFile? = nil) {
                self.name = name
                self.path = path
                self.isDirectory = isDirectory
                self.file = file
            }

            func toNode() -> FileTreeNode {
                FileTreeNode(
                    name: name,
                    path: path,
                    isDirectory: isDirectory,
                    children: children.values
                        .map { $0.toNode() }
                        .sorted { sortNodes($0, $1) },
                    file: file
                )
            }

            private func sortNodes(_ a: FileTreeNode, _ b: FileTreeNode) -> Bool {
                if a.isDirectory != b.isDirectory {
                    return a.isDirectory
                }
                return a.name.lowercased() < b.name.lowercased()
            }
        }

        let root = TreeBuilder(name: "", path: "", isDirectory: true)

        for file in files {
            let components = file.relativePath.components(separatedBy: "/")
            var current = root
            var currentPath = ""

            for (index, component) in components.enumerated() {
                let isLast = index == components.count - 1
                currentPath = currentPath.isEmpty ? component : "\(currentPath)/\(component)"

                if isLast {
                    // It's a file
                    current.children[component] = TreeBuilder(
                        name: component,
                        path: currentPath,
                        isDirectory: false,
                        file: file
                    )
                } else {
                    // It's a directory
                    if current.children[component] == nil {
                        current.children[component] = TreeBuilder(
                            name: component,
                            path: currentPath,
                            isDirectory: true
                        )
                    }
                    current = current.children[component]!
                }
            }
        }

        // Convert to nodes and return top-level children
        return root.children.values
            .map { $0.toNode() }
            .sorted {
                if $0.isDirectory != $1.isDirectory {
                    return $0.isDirectory
                }
                return $0.name.lowercased() < $1.name.lowercased()
            }
    }
}

// MARK: - Array Extension

private extension Array {
    subscript(safe index: Int) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}
