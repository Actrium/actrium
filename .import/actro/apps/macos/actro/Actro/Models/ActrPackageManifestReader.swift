import Foundation
import ActroCore

private let actrManifestLogger = AppLogger.viewModel

struct ActrPackageManifestInfo {
    let name: String?
    let version: String?
    let description: String?
    let appId: String?
    let icon: String?
    let contractName: String?
    let contractVersion: Int?
    let permissions: [String]
    let leftPanelDefaultVisible: Bool
    let rightPanelDefaultVisible: Bool
    let leftPanelScript: String?
    let rightPanelScript: String?
    let unifiedScript: String?
    let systemAppType: SystemTabType?
    let themeColor: String?
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
            icon: manifest.icon,
            contractName: manifest.contractName,
            contractVersion: manifest.contractVersion,
            permissions: manifest.permissions,
            leftPanelDefaultVisible: manifest.leftPanelDefaultVisible,
            rightPanelDefaultVisible: manifest.rightPanelDefaultVisible,
            leftPanelScript: manifest.leftPanelScript,
            rightPanelScript: manifest.rightPanelScript,
            unifiedScript: manifest.unifiedScript,
            systemAppType: manifest.systemAppType.flatMap(SystemTabType.init(rawValue:)),
            themeColor: manifest.themeColor
        )
    }

    private static func logManifest(_ manifest: AppleActrPackageManifestInfo) {
        if manifest.usesLegacyLayoutFallback {
            actrManifestLogger.warning(" [ActrPackageManifestReader] manifest 缺少 entry，按 legacy layout 格式加载")
        }
        if let color = manifest.themeColor {
            actrManifestLogger.notice("🎨 [ActrPackageManifestReader] 从 manifest 读取 themeColor: \(color)")
        }
        if let contractName = manifest.contractName,
           let contractVersion = manifest.contractVersion {
            actrManifestLogger.notice(" [ActrPackageManifestReader] contract: \(contractName)@v\(contractVersion)")
        } else if manifest.usesLegacyLayoutFallback {
            actrManifestLogger.warning(" [ActrPackageManifestReader] manifest.contract 字段不完整，跳过校验")
        } else {
            actrManifestLogger.warning(" [ActrPackageManifestReader] manifest 缺少 contract（兼容模式继续运行）")
        }
        if !manifest.permissions.isEmpty {
            actrManifestLogger.debug("🔐 [ActrPackageManifestReader] permissions: \(manifest.permissions.joined(separator: ", "))")
        }
        if let algorithm = manifest.integrityAlgorithm {
            actrManifestLogger.notice(" [ActrPackageManifestReader] ACTR integrity verified: \(manifest.integrityFileCount) files, \(algorithm)")
        }
    }
}
