import Foundation

enum ActrPackageExtractor {
    nonisolated static func isSafeRelativePath(_ path: String) -> Bool {
        !path.isEmpty && !path.hasPrefix("/") && !path.contains("\\") &&
            !path.contains("\0") && !path.split(separator: "/").contains("..")
    }

    /// Manifest paths and recursive fallback matches must stay inside the bounded
    /// extraction directory. Only regular files can be loaded as scripts.
    nonisolated static func containedFile(in root: URL, relativePath: String) -> URL? {
        guard isSafeRelativePath(relativePath) else { return nil }
        let canonicalRoot = root.resolvingSymlinksInPath().standardizedFileURL
        let candidate = canonicalRoot.appendingPathComponent(relativePath)
            .resolvingSymlinksInPath().standardizedFileURL
        guard candidate.path.hasPrefix(canonicalRoot.path + "/"),
              let values = try? candidate.resourceValues(forKeys: [.isRegularFileKey]),
              values.isRegularFile == true else {
            return nil
        }
        return candidate
    }

    /// Limits apply across the entire archive, including entries without data.
    nonisolated static func extract(
        archive: URL,
        maximumBytes: UInt64 = 512 * 1024 * 1024,
        maximumEntries: UInt64 = 10_000,
        temporaryDirectory: URL = FileManager.default.temporaryDirectory
    ) throws -> URL {
        let destination = temporaryDirectory
            .appendingPathComponent("actr-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(
            at: destination,
            withIntermediateDirectories: false,
            attributes: [.posixPermissions: 0o700]
        )
        do {
            var message = [CChar](repeating: 0, count: 256)
            let messageCapacity = message.count
            let result = actr_extract_zip(
                archive.path, destination.path, maximumBytes, maximumEntries,
                &message, messageCapacity
            )
            guard result == 0 else {
                throw NSError(
                    domain: "ActrPackageExtraction", code: Int(result),
                    userInfo: [NSLocalizedDescriptionKey: String(cString: message)]
                )
            }
            return destination
        } catch {
            try? FileManager.default.removeItem(at: destination)
            throw error
        }
    }
}
