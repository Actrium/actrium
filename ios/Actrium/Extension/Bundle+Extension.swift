import Foundation

extension Bundle {
    // Get an application number
    var appVersion: String {
        if let version = self.infoDictionary?["CFBundleShortVersionString"] as? String {
            return "V\(version)"
        }
        return "V1.0.0" // Default Version Number
    }
    
    // Get an application builder
    var appBuild: String {
        if let build = self.infoDictionary?["CFBundleVersion"] as? String {
            return build
        }
        return "1" // Default Build Number
    }
    
    // Get full version and build number
    var versionAndBuild: String {
        return "\(appVersion) (\(appBuild))"
    }
}