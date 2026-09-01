import Foundation
import CryptoKit

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
    public let usesLegacyLayoutFallback: Bool
    public let integrityAlgorithm: String?
    public let integrityFileCount: Int
}

public enum AppleActrPackageManifestReader {
    public static func read(fromRoot root: URL) throws -> AppleActrPackageManifestInfo {
        guard let manifestURL = findManifestJSON(in: root) else {
            throw NSError(
                domain: "ACTRPackage",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: "包内未找到 manifest.json"]
            )
        }

        let data = try Data(contentsOf: manifestURL)
        guard let manifestJSON = String(data: data, encoding: .utf8) else {
            throw NSError(
                domain: "ACTRPackage",
                code: 2,
                userInfo: [NSLocalizedDescriptionKey: "manifest.json 不是有效的 UTF-8 文本"]
            )
        }

        let json = try JSONSerialization.jsonObject(with: data, options: []) as? [String: Any]
        let layout = json?["layout"] as? [String: Any]
        let leftPanelScript = layout?["leftPanel"] as? String
        let rightPanelScript = layout?["rightPanel"] as? String

        do {
            let manifest = try parseActrManifest(json: manifestJSON)
            let integrity = try verifyACTRIntegrity(fromRoot: root, manifestJSON: manifestJSON)
            let contractVersion = manifest.contractVersion.map(Int.init)

            return AppleActrPackageManifestInfo(
                name: manifest.name,
                version: manifest.version,
                description: manifest.description,
                appId: manifest.appId,
                icon: manifest.icon,
                contractName: manifest.contractName,
                contractVersion: contractVersion,
                permissions: manifest.permissions,
                leftPanelDefaultVisible: manifest.leftPanelDefaultVisible,
                rightPanelDefaultVisible: manifest.rightPanelDefaultVisible,
                leftPanelScript: leftPanelScript,
                rightPanelScript: rightPanelScript,
                unifiedScript: manifest.entry,
                systemAppType: manifest.systemAppType,
                themeColor: manifest.themeColor,
                usesLegacyLayoutFallback: false,
                integrityAlgorithm: integrity?.algorithm,
                integrityFileCount: integrity?.fileCount ?? 0
            )
        } catch CoreError.MissingField(let field) where field == "entry" && (leftPanelScript != nil || rightPanelScript != nil) {
            return try readLegacyLayoutManifest(
                json: json,
                leftPanelScript: leftPanelScript,
                rightPanelScript: rightPanelScript
            )
        }
    }

    private static func readLegacyLayoutManifest(
        json: [String: Any]?,
        leftPanelScript: String?,
        rightPanelScript: String?
    ) throws -> AppleActrPackageManifestInfo {
        let name = json?["name"] as? String
        let version = json?["version"] as? String
        let description = json?["description"] as? String
        let appId = (json?["id"] as? String) ?? (json?["bundleId"] as? String)
        let icon = json?["icon"] as? String
        let themeColor = (json?["themeColor"] as? String) ?? (json?["backgroundColor"] as? String)
        let leftPanelDefaultVisible = json?["leftPanelDefaultVisible"] as? Bool ?? true
        let rightPanelDefaultVisible = json?["rightPanelDefaultVisible"] as? Bool ?? false
        let permissions = json?["permissions"] as? [String] ?? []
        let systemAppType = json?["systemAppType"] as? String

        var contractName: String? = nil
        var contractVersion: Int? = nil
        if let contract = json?["contract"] as? [String: Any] {
            contractName = contract["name"] as? String
            contractVersion = {
                if let value = contract["version"] as? Int { return value }
                if let value = contract["version"] as? Double { return Int(value) }
                return nil
            }()

            if let contractName, let contractVersion {
                let gate = evaluateContract(name: contractName, version: UInt32(contractVersion))
                if gate.status == .blocked {
                    throw NSError(
                        domain: "ACTRContract",
                        code: 1,
                        userInfo: [NSLocalizedDescriptionKey: gate.blockedReason ?? "不支持的 contract"]
                    )
                }
            }
        }

        return AppleActrPackageManifestInfo(
            name: name,
            version: version,
            description: description,
            appId: appId,
            icon: icon,
            contractName: contractName,
            contractVersion: contractVersion,
            permissions: permissions,
            leftPanelDefaultVisible: leftPanelDefaultVisible,
            rightPanelDefaultVisible: rightPanelDefaultVisible,
            leftPanelScript: leftPanelScript,
            rightPanelScript: rightPanelScript,
            unifiedScript: nil,
            systemAppType: systemAppType,
            themeColor: themeColor,
            usesLegacyLayoutFallback: true,
            integrityAlgorithm: nil,
            integrityFileCount: 0
        )
    }

    private static func verifyACTRIntegrity(
        fromRoot root: URL,
        manifestJSON: String
    ) throws -> (algorithm: String, fileCount: Int)? {
        guard let plan = try createActrIntegrityPlan(manifestJson: manifestJSON) else {
            return nil
        }

        for file in plan.files {
            let fileURL = try resolvedACTRIntegrityFileURL(root: root, path: file.path)
            let actualHash = try sha256Hex(for: fileURL)
            guard actualHash == file.expectedHash else {
                throw NSError(
                    domain: "ACTRIntegrity",
                    code: 3,
                    userInfo: [NSLocalizedDescriptionKey: "ACTR integrity check failed for \(file.path)"]
                )
            }
        }

        return (algorithm: plan.algorithm, fileCount: plan.files.count)
    }

    private static func resolvedACTRIntegrityFileURL(root: URL, path: String) throws -> URL {
        let rootURL = root.standardizedFileURL.resolvingSymlinksInPath()
        let fileURL = root
            .appendingPathComponent(path, isDirectory: false)
            .standardizedFileURL
            .resolvingSymlinksInPath()

        let rootPath = rootURL.path
        let filePath = fileURL.path
        guard filePath == rootPath || filePath.hasPrefix(rootPath + "/") else {
            throw NSError(
                domain: "ACTRIntegrity",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: "ACTR integrity path escapes package: \(path)"]
            )
        }

        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: filePath, isDirectory: &isDirectory),
              !isDirectory.boolValue else {
            throw NSError(
                domain: "ACTRIntegrity",
                code: 2,
                userInfo: [NSLocalizedDescriptionKey: "ACTR integrity file missing: \(path)"]
            )
        }

        return fileURL
    }

    private static func sha256Hex(for fileURL: URL) throws -> String {
        let handle = try FileHandle(forReadingFrom: fileURL)
        defer {
            try? handle.close()
        }

        var hasher = SHA256()
        while true {
            let chunk = try handle.read(upToCount: 1024 * 1024) ?? Data()
            if chunk.isEmpty {
                break
            }
            hasher.update(data: chunk)
        }

        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }

    private static func findManifestJSON(in root: URL) -> URL? {
        let fm = FileManager.default
        if let direct = try? fm.contentsOfDirectory(at: root, includingPropertiesForKeys: nil)
            .first(where: { $0.lastPathComponent == "manifest.json" }) {
            return direct
        }

        if let enumerator = fm.enumerator(at: root, includingPropertiesForKeys: nil) {
            for case let fileURL as URL in enumerator where fileURL.lastPathComponent == "manifest.json" {
                return fileURL
            }
        }
        return nil
    }
}

