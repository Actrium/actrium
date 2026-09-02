import Foundation

enum ActroAppURLScheme {
    static let primary = "actro"

    static func isSupported(_ scheme: String?) -> Bool {
        scheme == primary
    }

    static func hasSupportedPrefix(_ input: String) -> Bool {
        input.hasPrefix("\(primary)://")
    }

    static func appURL(bundleId: String) -> URL {
        URL(string: "\(primary)://\(bundleId)")!
    }

    static func appURL(filePath: String) -> URL? {
        URL(string: "\(primary)://\(filePath)")
    }
}

enum ActrPublicURLScheme {
    static let primary = "actr"

    static func isSupported(_ scheme: String?) -> Bool {
        scheme == primary
    }

    static func hasSupportedPrefix(_ input: String) -> Bool {
        input.hasPrefix("\(primary)://")
    }
}
