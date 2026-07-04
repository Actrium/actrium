import Foundation
import ActroCore

enum ActroAppIdentity {
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
        ActroAppIdentity.appId(explicit: appId, url: url)
    }
}
