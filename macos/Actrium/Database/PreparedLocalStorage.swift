import Foundation

/// Native-only result of storage preparation, bound to its verified principal.
struct PreparedLocalStorage {
    let principal: String
    let publisherIdentity: String
    let origin: String

    var configuration: [String: String] {
        ["principal": principal, "publisherIdentity": publisherIdentity, "origin": origin]
    }

    init(principal: String, publisherIdentity: String, origin: String) {
        self.principal = principal
        self.publisherIdentity = publisherIdentity
        self.origin = origin
    }

    init?(configuration: [String: String]) {
        guard let principal = configuration["principal"],
              let identity = configuration["publisherIdentity"], let origin = configuration["origin"] else { return nil }
        self.init(principal: principal, publisherIdentity: identity, origin: origin)
    }

    func matches(principal: String?, publisherIdentity: String?) -> Bool {
        self.principal == principal && self.publisherIdentity == publisherIdentity
    }
}
