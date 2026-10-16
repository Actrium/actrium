import Foundation
import ActriumCore

enum ActriumAppIdentity {
    static func appId(explicit: String?, url: URL?) -> String? {
        if let explicit, !explicit.isEmpty {
            return explicit
        }
        guard let url else { return nil }
        return extractAppIdFromUrl(url: url.absoluteString)
    }
}

extension Tab {
    var resolvedAppId: String? {
        ActriumAppIdentity.appId(explicit: appId, url: url)
    }
}
