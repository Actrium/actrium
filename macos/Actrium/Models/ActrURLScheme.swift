import Foundation

enum ActrPublicURLScheme {
    static let primary = "actr"

    static func isSupported(_ scheme: String?) -> Bool {
        scheme?.lowercased() == primary
    }

    static func hasSupportedPrefix(_ input: String) -> Bool {
        input.lowercased().hasPrefix("\(primary)://")
    }
}
