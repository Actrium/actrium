import Foundation

enum ActriumAppURLScheme {
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

    static func bundleId(from url: URL) -> String? {
        guard isSupported(url.scheme), let host = url.host, !host.isEmpty else { return nil }
        return url.path == "/" ? host : host + url.path
    }

    static func appURL(filePath: String) -> URL? {
        URL(string: "\(primary)://\(filePath)")
    }
}
