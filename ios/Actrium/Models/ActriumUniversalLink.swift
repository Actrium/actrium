import Foundation

enum ActriumUniversalLink {
    static let hosts: Set<String> = [
        "go.actro.chat",
        "go-actro.betaenv.com",
        "go-actro.develenv.com"
    ]

    static func appId(from url: URL) -> String? {
        guard let host = url.host?.lowercased(),
              hosts.contains(host) else {
            return nil
        }

        let appId = URLComponents(url: url, resolvingAgainstBaseURL: true)?
            .queryItems?
            .first { $0.name.lowercased() == "appid" }?
            .value

        guard let appId, !appId.isEmpty else { return nil }
        return appId
    }
}
