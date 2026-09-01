import Foundation
import ActroCore

private let actrManifestLogger = AppLogger.viewModel

struct ActrPackageManifestInfo {
    let name: String?
    let version: String?
    let description: String?
    let appId: String?
    let contractName: String?
    let contractVersion: Int?
    let permissions: [String]
    let leftPanelDefaultVisible: Bool
    let rightPanelDefaultVisible: Bool
    let leftPanelScript: String?
    let rightPanelScript: String?
    let unifiedScript: String?
}

enum ActrPackageManifestReader {
    static func read(fromRoot root: URL) throws -> ActrPackageManifestInfo {
        let manifest = try AppleActrPackageManifestReader.read(fromRoot: root)
        logManifest(manifest)
        return ActrPackageManifestInfo(
            name: manifest.name,
            version: manifest.version,
            description: manifest.description,
            appId: manifest.appId,
            contractName: manifest.contractName,
            contractVersion: manifest.contractVersion,
            permissions: manifest.permissions,
            leftPanelDefaultVisible: manifest.leftPanelDefaultVisible,
            rightPanelDefaultVisible: manifest.rightPanelDefaultVisible,
            leftPanelScript: manifest.leftPanelScript,
            rightPanelScript: manifest.rightPanelScript,
            unifiedScript: manifest.unifiedScript
        )
    }

    private static func logManifest(_ manifest: AppleActrPackageManifestInfo) {
        if let contractName = manifest.contractName,
           let contractVersion = manifest.contractVersion {
            actrManifestLogger.notice("[ActrPackageManifestReader] workload contract: \(contractName)@v\(contractVersion)")
        }
        if !manifest.permissions.isEmpty {
            actrManifestLogger.debug("[ActrPackageManifestReader] capabilities: \(manifest.permissions.joined(separator: ", "))")
        }
        actrManifestLogger.notice("[ActrPackageManifestReader] payload integrity verified: \(manifest.integrityFileCount) files, \(manifest.integrityAlgorithm)")
    }
}
