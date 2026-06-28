import Foundation

/// Service for interacting with Metro bundler
actor MetroService {
    static let shared = MetroService()

    private var cachedSourceMap: SourceMap?
    private var cachedAssets: [SourceFile]?
    private var lastFetchTime: Date?
    private let cacheTimeout: TimeInterval = 30 // Refresh every 30 seconds

    private init() {}

    /// Asset file extensions to scan for
    private let assetExtensions = ["png", "jpg", "jpeg", "gif", "svg", "webp", "ico", "ttf", "otf", "woff", "woff2", "mp3", "wav", "mp4", "json"]

    private func defaultMetroPort() -> Int {
        if let env = ProcessInfo.processInfo.environment["RCT_METRO_PORT"], let port = Int(env) {
            return port
        }
        return 8081
    }

    private func metroHost(_ port: Int) -> String {
        // 使用 127.0.0.1 避免 mDNS / localhost 解析受限
        return "127.0.0.1:\(port)"
    }

    /// Fetch source map from Metro
    func fetchSourceMap(port: Int? = nil) async throws -> SourceMap {
        // Return cached if still valid
        if let cached = cachedSourceMap,
           let lastFetch = lastFetchTime,
           Date().timeIntervalSince(lastFetch) < cacheTimeout {
            return cached
        }

        let finalPort = port ?? defaultMetroPort()
        let host = metroHost(finalPort)
        let url = URL(string: "http://\(host)/index.map?platform=macos&dev=true")!
        let (data, _) = try await URLSession.shared.data(from: url)
        let sourceMap = try JSONDecoder().decode(SourceMap.self, from: data)

        cachedSourceMap = sourceMap
        lastFetchTime = Date()

        return sourceMap
    }

    /// Get list of project source files (excluding node_modules)
    func getProjectFiles(port: Int? = nil) async throws -> [SourceFile] {
        let sourceMap = try await fetchSourceMap(port: port)
        var files = sourceMap.projectFiles

        // Also scan for assets in project directories
        let assets = scanProjectAssets(from: files)
        files.append(contentsOf: assets)

        return files
    }

    /// Get source content for a specific file
    func getSourceContent(for path: String, port: Int? = nil) async throws -> String? {
        let sourceMap = try await fetchSourceMap(port: port)
        return sourceMap.getContent(for: path)
    }

    /// Invalidate cache
    func invalidateCache() {
        cachedSourceMap = nil
        cachedAssets = nil
        lastFetchTime = nil
    }

    /// Resolve bundle line/column to original source position using source map
    func resolveOriginalPosition(bundleLine: Int, bundleColumn: Int, port: Int? = nil) async throws -> (source: String, line: Int, column: Int)? {
        let sourceMap = try await fetchSourceMap(port: port)
        return sourceMap.resolveOriginalPosition(bundleLine: bundleLine, bundleColumn: bundleColumn)
    }

    /// Scan for asset files in project directories
    private func scanProjectAssets(from sourceFiles: [SourceFile]) -> [SourceFile] {
        // Return cached if available
        if let cached = cachedAssets {
            return cached
        }

        // Find project root from source paths
        guard let projectRoot = findProjectRoot(from: sourceFiles) else {
            return []
        }

        var assets: [SourceFile] = []
        let fileManager = FileManager.default

        // Common asset directories to scan
        let assetDirs = [
            "assets",
            "images",
            "fonts",
            "src/assets",
            "src/images",
            "RCTNApps/assets",
            "RCTNApps/src/assets"
        ]

        for dir in assetDirs {
            let dirPath = (projectRoot as NSString).appendingPathComponent(dir)
            if let files = try? fileManager.contentsOfDirectory(atPath: dirPath) {
                for file in files {
                    let filePath = (dirPath as NSString).appendingPathComponent(file)
                    let ext = (file as NSString).pathExtension.lowercased()
                    if assetExtensions.contains(ext) {
                        assets.append(SourceFile(
                            fullPath: filePath,
                            content: nil,
                            isAsset: true
                        ))
                    }
                }
            }

            // Also scan subdirectories one level deep
            if let subdirs = try? fileManager.contentsOfDirectory(atPath: dirPath) {
                for subdir in subdirs {
                    let subdirPath = (dirPath as NSString).appendingPathComponent(subdir)
                    var isDir: ObjCBool = false
                    if fileManager.fileExists(atPath: subdirPath, isDirectory: &isDir), isDir.boolValue {
                        if let files = try? fileManager.contentsOfDirectory(atPath: subdirPath) {
                            for file in files {
                                let filePath = (subdirPath as NSString).appendingPathComponent(file)
                                let ext = (file as NSString).pathExtension.lowercased()
                                if assetExtensions.contains(ext) {
                                    assets.append(SourceFile(
                                        fullPath: filePath,
                                        content: nil,
                                        isAsset: true
                                    ))
                                }
                            }
                        }
                    }
                }
            }
        }

        cachedAssets = assets
        return assets
    }

    /// Find project root directory from source file paths
    private func findProjectRoot(from files: [SourceFile]) -> String? {
        // Look for common project markers in paths
        for file in files {
            if let range = file.fullPath.range(of: "/RCTNApps/") {
                let root = String(file.fullPath[..<range.lowerBound])
                // Go up one level from RCTNApps to get Actro project root
                return (root as NSString).appendingPathComponent("RCTNApps")
            }
        }
        return nil
    }
}

// MARK: - Source Map Model

struct SourceMap: Decodable, Sendable {
    let version: Int
    let sources: [String]
    let sourcesContent: [String?]?
    let names: [String]?
    let mappings: String

    /// Get all source files from source map (no filtering)
    var projectFiles: [SourceFile] {
        var files: [SourceFile] = []
        for (index, path) in sources.enumerated() {
            // Skip internal/polyfill files only
            guard !path.starts(with: "__") else { continue }

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

    /// Resolve bundle line/column to original source file and line
    /// Returns (sourceFile, line, column) if found
    func resolveOriginalPosition(bundleLine: Int, bundleColumn: Int) -> (source: String, line: Int, column: Int)? {
        // Parse VLQ mappings
        let segments = parseMappings()

        // Find the mapping for the given bundle line/column
        // Segments are organized by output (bundle) line
        guard bundleLine > 0, bundleLine <= segments.count else { return nil }

        let lineSegments = segments[bundleLine - 1]

        // Find the segment that covers this column
        var bestMatch: MappingSegment?
        for segment in lineSegments {
            if segment.generatedColumn <= bundleColumn {
                bestMatch = segment
            } else {
                break // Segments are sorted by column
            }
        }

        guard let match = bestMatch else { return nil }
        guard match.sourceIndex >= 0, match.sourceIndex < sources.count else { return nil }

        let sourcePath = sources[match.sourceIndex]
        return (sourcePath, match.sourceLine + 1, match.sourceColumn)
    }

    /// Parse VLQ-encoded mappings into segments
    private func parseMappings() -> [[MappingSegment]] {
        var result: [[MappingSegment]] = []
        var currentLine: [MappingSegment] = []

        // State carried across segments
        var generatedColumn = 0
        var sourceIndex = 0
        var sourceLine = 0
        var sourceColumn = 0
        var nameIndex = 0

        for line in mappings.split(separator: ";", omittingEmptySubsequences: false) {
            generatedColumn = 0 // Reset for each line
            currentLine = []

            if !line.isEmpty {
                for segment in line.split(separator: ",") {
                    let values = decodeVLQ(String(segment))

                    guard values.count >= 1 else { continue }

                    generatedColumn += values[0]

                    if values.count >= 4 {
                        sourceIndex += values[1]
                        sourceLine += values[2]
                        sourceColumn += values[3]

                        if values.count >= 5 {
                            nameIndex += values[4]
                        }

                        currentLine.append(MappingSegment(
                            generatedColumn: generatedColumn,
                            sourceIndex: sourceIndex,
                            sourceLine: sourceLine,
                            sourceColumn: sourceColumn
                        ))
                    }
                }
            }

            result.append(currentLine)
        }

        return result
    }

    /// Decode a VLQ-encoded segment into an array of integers
    private func decodeVLQ(_ str: String) -> [Int] {
        let base64Chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
        var result: [Int] = []
        var shift = 0
        var value = 0

        for char in str {
            guard let index = base64Chars.firstIndex(of: char) else { continue }
            let digit = base64Chars.distance(from: base64Chars.startIndex, to: index)

            let hasMore = (digit & 32) != 0
            value += (digit & 31) << shift

            if hasMore {
                shift += 5
            } else {
                // Convert from unsigned to signed
                let isNegative = (value & 1) != 0
                value >>= 1
                if isNegative {
                    value = -value
                }
                result.append(value)
                value = 0
                shift = 0
            }
        }

        return result
    }
}

/// A single mapping segment
private struct MappingSegment {
    let generatedColumn: Int
    let sourceIndex: Int
    let sourceLine: Int
    let sourceColumn: Int
}

/// Represents a source file from the bundle
struct SourceFile: Identifiable, Hashable, Sendable {
    let id = UUID()
    let fullPath: String
    let content: String?
    let isAsset: Bool

    init(fullPath: String, content: String?, isAsset: Bool = false) {
        self.fullPath = fullPath
        self.content = content
        self.isAsset = isAsset
    }

    /// File name only
    var name: String {
        (fullPath as NSString).lastPathComponent
    }

    /// Directory path
    var directory: String {
        (fullPath as NSString).deletingLastPathComponent
    }

    /// Relative path for file tree display
    /// Two fixed root directories: Host (actr-host) and Guest (embedded app)
    var relativePath: String {
        // Host = actr-host directory (the host application code)
        if let range = fullPath.range(of: "/actr-host/") {
            return "Host/" + String(fullPath[range.upperBound...])
        }
        // Also include RCTNApps-level files under Host
        if let range = fullPath.range(of: "/RCTNApps/src/") {
            let remainder = String(fullPath[range.upperBound...])
            // Files directly in RCTNApps/src but not in actr-host
            if !remainder.contains("actr-host") {
                return "Host/" + remainder
            }
        }
        // Guest = countapp or any guest app directory
        if let range = fullPath.range(of: "/countapp/") {
            return "Guest/" + String(fullPath[range.upperBound...])
        }
        // For other paths, show full relative path from a recognizable root
        if let range = fullPath.range(of: "/node_modules/") {
            return "node_modules/" + String(fullPath[range.upperBound...])
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

    /// Whether this is an image file
    var isImage: Bool {
        let imageExtensions = ["png", "jpg", "jpeg", "gif", "svg", "webp", "ico"]
        return imageExtensions.contains(fileExtension.lowercased())
    }

    /// Whether this is a font file
    var isFont: Bool {
        let fontExtensions = ["ttf", "otf", "woff", "woff2"]
        return fontExtensions.contains(fileExtension.lowercased())
    }

    /// Language for syntax highlighting
    var language: String {
        switch fileExtension.lowercased() {
        case "ts", "tsx": return "typescript"
        case "js", "jsx": return "javascript"
        case "json": return "json"
        case "css": return "css"
        case "html": return "html"
        case "xml", "svg": return "xml"
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
