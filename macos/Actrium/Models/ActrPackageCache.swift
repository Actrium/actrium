import Foundation

/// Digest-addressed package cache (05 §15):
///
///     <root>/packages/sha256/<digest>/package.actr
///     <root>/extracted/sha256/<digest>/...
///
/// Rules (05 §15):
/// - the original archive is kept for re-verification;
/// - extracted files are produced only from the archived package;
/// - writes are atomic (staging dir, then rename into place);
/// - unfinished work stays in staging paths, never at the final path.
///
/// Cached packages are capped at 2 GiB and evicted least recently used first.
nonisolated enum ActrPackageCache {
    /// Cache root scoped to a test and the tasks it starts.
    @TaskLocal static var rootOverrideForTesting: URL?

    static let maxCacheBytes: Int64 = 2 * 1024 * 1024 * 1024

    /// Serializes installs, extraction, eviction, cleanup, and URL-map updates.
    /// Recursive because eviction re-enters size and last-used helpers.
    private static let cacheLock = NSRecursiveLock()

    private static func withCacheLock<T>(_ body: () throws -> T) rethrows -> T {
        cacheLock.lock()
        defer { cacheLock.unlock() }
        return try body()
    }

    /// Cache root: ~/Library/Caches/<bundle-id>/ (system-managed, purgeable).
    static var root: URL {
        if let rootOverrideForTesting { return rootOverrideForTesting }
        let base = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first
            ?? FileManager.default.temporaryDirectory
        let bundleID = Bundle.main.bundleIdentifier ?? "io.actrium.actro"
        return base.appendingPathComponent(bundleID, isDirectory: true)
    }

    private static var packagesRoot: URL {
        root.appendingPathComponent("packages/sha256", isDirectory: true)
    }

    private static var extractedRoot: URL {
        root.appendingPathComponent("extracted/sha256", isDirectory: true)
    }

    // MARK: - Paths

    /// Digests reach the cache from user-writable state (URL memo, instance
    /// store) and are interpolated into filesystem paths, so only the exact
    /// lowercase-hex sha256 form is accepted.
    private static let digestPattern = try! NSRegularExpression(pattern: "^[0-9a-f]{64}$")

    static func isValidDigest(_ digest: String) -> Bool {
        let range = NSRange(digest.startIndex..., in: digest)
        return digestPattern.firstMatch(in: digest, range: range) != nil
    }

    /// Digest-addressed path of the archived package.
    static func archiveURL(forDigest digest: String) -> URL {
        precondition(isValidDigest(digest), "cache paths require a 64-char lowercase hex digest")
        return root
            .appendingPathComponent("packages/sha256/\(digest)", isDirectory: true)
            .appendingPathComponent("package.actr")
    }

    /// Digest-addressed extraction directory.
    static func extractedURL(forDigest digest: String) -> URL {
        precondition(isValidDigest(digest), "cache paths require a 64-char lowercase hex digest")
        return extractedRoot.appendingPathComponent(digest, isDirectory: true)
    }

    private static func lastUsedURL(forDigest digest: String) -> URL {
        archiveURL(forDigest: digest)
            .deletingLastPathComponent()
            .appendingPathComponent("last-used", isDirectory: false)
    }

    static func touchLastUsed(forDigest digest: String) {
        withCacheLock {
            let file = lastUsedURL(forDigest: digest)
            try? FileManager.default.createDirectory(
                at: file.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            let timestamp = String(Int64(Date().timeIntervalSince1970))
            try? Data(timestamp.utf8).write(to: file, options: .atomic)
        }
    }

    static func lastUsed(forDigest digest: String) -> Date {
        withCacheLock {
            let lastUsedFile = lastUsedURL(forDigest: digest)
            if let data = try? Data(contentsOf: lastUsedFile),
               let timestamp = String(data: data, encoding: .utf8).flatMap(TimeInterval.init) {
                return Date(timeIntervalSince1970: timestamp)
            }
            if let attributes = try? FileManager.default.attributesOfItem(
                atPath: archiveURL(forDigest: digest).path
            ), let modificationDate = attributes[.modificationDate] as? Date {
                return modificationDate
            }
            return .distantPast
        }
    }

    static func digest(fromExtractedPath path: String) -> String? {
        let candidate = URL(fileURLWithPath: path, isDirectory: true).standardizedFileURL
        let digest = candidate.lastPathComponent
        guard isValidDigest(digest),
              candidate == extractedURL(forDigest: digest).standardizedFileURL
        else { return nil }
        return digest
    }

    // MARK: - Archive install

    /// Copies an archive into its digest-addressed location; reuses an existing
    /// copy (same digest implies identical content). Returns the cached path.
    /// Use `replacingRejectedArchive` with verified bytes to repair a failed cache check.
    static func installArchive(at sourceArchiveURL: URL, digest: String, replacingRejectedArchive: Bool = false) throws -> URL {
        try withCacheLock {
            let fm = FileManager.default
            let destination = Self.archiveURL(forDigest: digest)
            if !replacingRejectedArchive, fm.fileExists(atPath: destination.path) {
                touchLastUsed(forDigest: digest)
                return destination
            }

            let destinationDir = destination.deletingLastPathComponent()
            try fm.createDirectory(at: destinationDir, withIntermediateDirectories: true)
            let staging = destinationDir
                .appendingPathComponent(".staging-\(UUID().uuidString)", isDirectory: false)
            defer { try? fm.removeItem(at: staging) }

            do {
                try fm.copyItem(at: sourceArchiveURL, to: staging)
                if replacingRejectedArchive, fm.fileExists(atPath: destination.path) {
                    let extracted = Self.extractedURL(forDigest: digest)
                    if fm.fileExists(atPath: extracted.path) { try fm.removeItem(at: extracted) }
                    _ = try fm.replaceItemAt(destination, withItemAt: staging)
                } else {
                    try fm.moveItem(at: staging, to: destination)
                }
            } catch {
                // Another writer landed the same digest first — reuse it.
                guard !replacingRejectedArchive, fm.fileExists(atPath: destination.path) else { throw error }
            }
            touchLastUsed(forDigest: digest)
            return destination
        }
    }

    // MARK: - Extraction

    /// Extracts a cached archive into its digest-addressed directory; reuses an
    /// existing extraction (same digest = same content, no re-extract on reopen).
    ///
    /// `validate` runs against the staging directory *before* it is moved to
    /// the digest-addressed path, so unverified content never becomes the
    /// cached extraction (05 §15: extracted files only come from verified
    /// packages). A hit on an existing extraction is re-validated as well,
    /// since the cache directory is not a trust boundary on its own.
    static func extractArchiveIfNeeded(
        _ archiveURL: URL,
        digest: String,
        validate: ((URL) throws -> Void)? = nil
    ) throws -> URL {
        try withCacheLock {
            let fm = FileManager.default
            let destination = extractedURL(forDigest: digest)
            if fm.fileExists(atPath: destination.path) {
                do {
                    try validate?(destination)
                    touchLastUsed(forDigest: digest)
                    return destination
                } catch {
                    // A damaged cache entry must not permanently poison future loads.
                    try fm.removeItem(at: destination)
                }
            }

            let parent = destination.deletingLastPathComponent()
            try fm.createDirectory(at: parent, withIntermediateDirectories: true)
            let staging = parent
                .appendingPathComponent(".staging-\(UUID().uuidString)", isDirectory: true)
            do {
                try fm.createDirectory(at: staging, withIntermediateDirectories: true)
                try unzipArchive(archiveURL, to: staging)
                try validate?(staging)
                do {
                    try fm.moveItem(at: staging, to: destination)
                } catch {
                    guard fm.fileExists(atPath: destination.path) else { throw error }
                    try? fm.removeItem(at: staging)
                }
            } catch {
                try? fm.removeItem(at: staging)
                throw error
            }
            touchLastUsed(forDigest: digest)
            return destination
        }
    }

    // MARK: - Size and eviction

    /// Recursively measures archived packages and extracted files. Call off the
    /// main actor for user-interface workflows.
    static func cacheSize() -> Int64 {
        withCacheLock {
            directorySize(at: packagesRoot) + directorySize(at: extractedRoot)
        }
    }

    /// Evicts least recently used cache entries until the cache is within the
    /// requested limit. Open-package digests are never candidates.
    static func evictIfNeeded(
        protectedDigests: Set<String>,
        maxBytes: Int64 = maxCacheBytes
    ) -> Int64 {
        withCacheLock {
            var remainingBytes = cacheSize()
            guard remainingBytes > maxBytes else { return 0 }

            let candidates = cachedDigests()
                .subtracting(protectedDigests)
                .sorted {
                    let leftDate = lastUsed(forDigest: $0)
                    let rightDate = lastUsed(forDigest: $1)
                    return leftDate == rightDate ? $0 < $1 : leftDate < rightDate
                }

            var freedBytes: Int64 = 0
            for digest in candidates where remainingBytes > maxBytes {
                let removed = removeCachedPackage(digest: digest)
                freedBytes += removed
                remainingBytes -= removed
            }
            return freedBytes
        }
    }

    /// Removes every unprotected cache entry and any interrupted staging work.
    @discardableResult
    static func clearAll(protectedDigests: Set<String>) -> Int64 {
        withCacheLock {
            var freedBytes: Int64 = 0
            for digest in cachedDigests().subtracting(protectedDigests) {
                freedBytes += removeCachedPackage(digest: digest)
            }
            freedBytes += removeStagingEntries()
            return freedBytes
        }
    }

    private static func cachedDigests() -> Set<String> {
        let fm = FileManager.default
        var digests = Set<String>()
        for base in [packagesRoot, extractedRoot] {
            guard let entries = try? fm.contentsOfDirectory(
                at: base,
                includingPropertiesForKeys: [.isDirectoryKey]
            ) else { continue }
            for entry in entries {
                let digest = entry.lastPathComponent
                guard isValidDigest(digest),
                      (try? entry.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true
                else { continue }
                digests.insert(digest)
            }
        }
        return digests
    }

    /// Deletes the archive, extraction, and matching URL memo entries as one
    /// cache-entry operation. Individual filesystem failures do not stop a sweep.
    private static func removeCachedPackage(digest: String) -> Int64 {
        let fm = FileManager.default
        let packageDirectory = archiveURL(forDigest: digest).deletingLastPathComponent()
        let extractedDirectory = extractedURL(forDigest: digest)
        let directories = [packageDirectory, extractedDirectory]
        let sizeBefore = directories.reduce(Int64(0)) { $0 + directorySize(at: $1) }

        for directory in directories where fm.fileExists(atPath: directory.path) {
            try? fm.removeItem(at: directory)
        }

        let sizeAfter = directories.reduce(Int64(0)) { $0 + directorySize(at: $1) }
        if !fm.fileExists(atPath: packageDirectory.path),
           !fm.fileExists(atPath: extractedDirectory.path) {
            let map = urlMap()
            let filtered = map.filter { $0.value["digest"] != digest }
            if filtered.count != map.count { saveURLMap(filtered) }
        }
        return max(0, sizeBefore - sizeAfter)
    }

    private static func removeStagingEntries() -> Int64 {
        let fm = FileManager.default
        var stagingEntries: [URL] = []
        for base in [packagesRoot, extractedRoot] {
            guard let enumerator = fm.enumerator(at: base, includingPropertiesForKeys: nil)
            else { continue }
            for case let entry as URL in enumerator where entry.lastPathComponent.hasPrefix(".staging-") {
                stagingEntries.append(entry)
            }
        }

        var freedBytes: Int64 = 0
        for entry in stagingEntries.sorted(by: { $0.path.count > $1.path.count }) {
            guard fm.fileExists(atPath: entry.path) else { continue }
            let size = directorySize(at: entry)
            do {
                try fm.removeItem(at: entry)
                freedBytes += size
            } catch {
                continue
            }
        }
        return freedBytes
    }

    private static func directorySize(at url: URL) -> Int64 {
        let fm = FileManager.default
        var isDirectory: ObjCBool = false
        guard fm.fileExists(atPath: url.path, isDirectory: &isDirectory) else { return 0 }
        guard isDirectory.boolValue else {
            let values = try? url.resourceValues(forKeys: [.fileSizeKey])
            return Int64(values?.fileSize ?? 0)
        }
        guard let enumerator = fm.enumerator(
            at: url,
            includingPropertiesForKeys: [.isRegularFileKey, .fileSizeKey]
        ) else { return 0 }

        var total: Int64 = 0
        for case let entry as URL in enumerator {
            guard let values = try? entry.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey]),
                  values.isRegularFile == true
            else { continue }
            total += Int64(values.fileSize ?? 0)
        }
        return total
    }

    // MARK: - Direct-link URL memo (B2: no re-download for a known link)

    /// Returns the last accepted digest for a direct link, when the
    /// cached archive for it still exists. Falls through to download otherwise.
    static func cachedDigest(forDownloadURL url: URL) -> String? {
        withCacheLock {
            guard let record = urlMap()[url.absoluteString], let digest = record["digest"],
                  isValidDigest(digest),
                  FileManager.default.fileExists(atPath: archiveURL(forDigest: digest).path)
            else { return nil }
            return digest
        }
    }

    /// Remembers which accepted digest a direct link resolved to.
    static func rememberDownloadURL(_ url: URL, digest: String) {
        withCacheLock {
            var map = urlMap()
            map[url.absoluteString] = [
                "digest": digest,
                "installedAtUnix": String(Date().timeIntervalSince1970),
            ]
            saveURLMap(map)
        }
    }

    static func forgetDownloadURL(_ url: URL, matchingDigest digest: String) {
        withCacheLock {
            var map = urlMap()
            guard map[url.absoluteString]?["digest"] == digest else { return }
            map.removeValue(forKey: url.absoluteString)
            saveURLMap(map)
        }
    }

    private static func urlMapFile() -> URL {
        root.appendingPathComponent("download-url-map.json")
    }

    private static func urlMap() -> [String: [String: String]] {
        guard let data = try? Data(contentsOf: urlMapFile()),
              let map = try? JSONSerialization.jsonObject(with: data) as? [String: [String: String]]
        else { return [:] }
        return map
    }

    private static func saveURLMap(_ map: [String: [String: String]]) {
        let file = urlMapFile()
        try? FileManager.default.createDirectory(
            at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        guard let data = try? JSONSerialization.data(withJSONObject: map) else { return }
        try? data.write(to: file, options: .atomic)
    }

    // MARK: - Unzip

    /// Unzips an .actr archive with /usr/bin/ditto (reliable macOS unzipping).
    static func unzipArchive(_ archive: URL, to destination: URL) throws {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/ditto")
        process.arguments = ["-xk", archive.path, destination.path]

        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = pipe

        try process.run()
        process.waitUntilExit()

        if process.terminationStatus != 0 {
            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            let output = String(data: data, encoding: .utf8) ?? ""
            throw NSError(
                domain: "ACTRUnzipError",
                code: Int(process.terminationStatus),
                userInfo: [NSLocalizedDescriptionKey: "Unzip failed: \(output)"]
            )
        }
    }
}
