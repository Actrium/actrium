import CryptoKit
import Foundation

public struct AppleActrPackageManifestInfo {
    public let name: String?
    public let version: String?
    public let description: String?
    public let appId: String?
    public let icon: String?
    public let contractName: String?
    public let contractVersion: Int?
    public let permissions: [String]
    public let leftPanelDefaultVisible: Bool
    public let rightPanelDefaultVisible: Bool
    public let leftPanelScript: String?
    public let rightPanelScript: String?
    public let unifiedScript: String?
    public let systemAppType: String?
    public let themeColor: String?
    public let integrityAlgorithm: String
    public let integrityFileCount: Int
}

public enum AppleActrPackageManifestReader {
    public static func read(fromRoot root: URL) throws -> AppleActrPackageManifestInfo {
        guard let manifestURL = findManifest(in: root) else {
            throw packageError(code: 1, message: "manifest.toml was not found")
        }
        let source = try String(contentsOf: manifestURL, encoding: .utf8)
        let manifest = try parseActrPackageManifest(source: source)
        let plan = try createActrIntegrityPlan(manifestToml: source)
        try verifyPayloads(fromRoot: manifestURL.deletingLastPathComponent(), plan: plan)

        return AppleActrPackageManifestInfo(
            name: manifest.name,
            version: manifest.version,
            description: manifest.description,
            appId: manifest.appId,
            icon: manifest.icon,
            contractName: manifest.contractName,
            contractVersion: manifest.contractVersion.map(Int.init),
            permissions: manifest.permissions,
            leftPanelDefaultVisible: manifest.leftPanelDefaultVisible,
            rightPanelDefaultVisible: manifest.rightPanelDefaultVisible,
            leftPanelScript: nil,
            rightPanelScript: nil,
            unifiedScript: manifest.entry,
            systemAppType: manifest.systemAppType,
            themeColor: manifest.themeColor,
            integrityAlgorithm: plan?.algorithm ?? "sha256",
            integrityFileCount: plan?.files.count ?? 0
        )
    }

    private static func verifyPayloads(
        fromRoot root: URL,
        plan: PackageIntegrityPlanInfo?
    ) throws {
        guard let plan else {
            throw packageError(code: 2, message: "the package does not declare payload files")
        }
        for file in plan.files {
            let fileURL = try resolvedPayloadURL(root: root, path: file.path)
            let actualHash = try sha256Hex(for: fileURL)
            guard actualHash == file.expectedHash else {
                throw packageError(
                    code: 3,
                    message: "payload digest mismatch for \(file.path)"
                )
            }
        }
    }

    private static func resolvedPayloadURL(root: URL, path: String) throws -> URL {
        let rootURL = root.standardizedFileURL.resolvingSymlinksInPath()
        let fileURL = root
            .appendingPathComponent(path, isDirectory: false)
            .standardizedFileURL
            .resolvingSymlinksInPath()
        let rootPath = rootURL.path
        let filePath = fileURL.path
        guard filePath.hasPrefix(rootPath + "/") else {
            throw packageError(code: 4, message: "payload path escapes package: \(path)")
        }
        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: filePath, isDirectory: &isDirectory),
              !isDirectory.boolValue else {
            throw packageError(code: 5, message: "payload file is missing: \(path)")
        }
        return fileURL
    }

    private static func sha256Hex(for fileURL: URL) throws -> String {
        let handle = try FileHandle(forReadingFrom: fileURL)
        defer { try? handle.close() }
        var hasher = SHA256()
        while true {
            let chunk = try handle.read(upToCount: 1024 * 1024) ?? Data()
            if chunk.isEmpty { break }
            hasher.update(data: chunk)
        }
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }

    private static func findManifest(in root: URL) -> URL? {
        let direct = root.appendingPathComponent("manifest.toml", isDirectory: false)
        if FileManager.default.fileExists(atPath: direct.path) {
            return direct
        }
        if let enumerator = FileManager.default.enumerator(
            at: root,
            includingPropertiesForKeys: nil
        ) {
            for case let fileURL as URL in enumerator
            where fileURL.lastPathComponent == "manifest.toml" {
                return fileURL
            }
        }
        return nil
    }

    private static func packageError(code: Int, message: String) -> NSError {
        NSError(
            domain: "ActrPackage",
            code: code,
            userInfo: [NSLocalizedDescriptionKey: message]
        )
    }
}
