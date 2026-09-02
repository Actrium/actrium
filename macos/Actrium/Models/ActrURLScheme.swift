import Foundation

enum ActrPublicURLScheme {
    static let primary = "actr"

    static func isSupported(_ scheme: String?) -> Bool {
        scheme == primary
    }

    static func hasSupportedPrefix(_ input: String) -> Bool {
        input.hasPrefix("\(primary)://")
    }
}
