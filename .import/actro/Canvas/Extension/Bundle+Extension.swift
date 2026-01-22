import Foundation

extension Bundle {
    // 获取应用版本号
    var appVersion: String {
        if let version = self.infoDictionary?["CFBundleShortVersionString"] as? String {
            return "V\(version)"
        }
        return "V1.0.0" // 默认版本号
    }
    
    // 获取应用构建号
    var appBuild: String {
        if let build = self.infoDictionary?["CFBundleVersion"] as? String {
            return build
        }
        return "1" // 默认构建号
    }
    
    // 获取完整的版本和构建号
    var versionAndBuild: String {
        return "\(appVersion) (\(appBuild))"
    }
}