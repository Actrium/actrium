import Foundation

/// Invite code carried in an `actr://` link's `args` query parameter.
///
/// `args` holds a percent-encoded JSON object such as
/// `{"invite":"ri_…"}`. The link keeps the manual invite sheet whenever the
/// parameter is absent, is not a JSON object, or carries an empty invite.
///
/// The invite is a single-use secret, so `strippingInviteArgs(from:)` removes
/// the parameter before an address reaches a tab, the restored session, or
/// history: the link must not outlive the redemption that consumed it.
enum ActrLinkInvite {
    static let argsName = "args"
    static let inviteKey = "invite"

    /// Invite carried in `uri`, or nil when the link has none usable.
    static func invite(from uri: String) -> String? {
        guard let args = firstArgsValue(in: uri),
              let data = args.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let invite = object[inviteKey] as? String
        else { return nil }
        let trimmed = invite.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    /// `uri` without its `args` parameter; other parameters are preserved.
    /// A link that cannot be parsed cannot supply a safe address.
    static func strippingInviteArgs(from uri: String) -> String? {
        guard var components = URLComponents(string: uri), components.url != nil else { return nil }
        guard components.percentEncodedQuery != nil else { return uri }
        guard let items = components.queryItems else { return nil }
        guard items.contains(where: { $0.name == argsName }) else { return uri }
        let remaining = items.filter { $0.name != argsName }
        components.queryItems = remaining.isEmpty ? nil : remaining
        return components.url?.absoluteString
    }

    /// Only the first `args` item counts, matching query-string precedence.
    private static func firstArgsValue(in uri: String) -> String? {
        URLComponents(string: uri)?
            .queryItems?
            .first(where: { $0.name == argsName })?
            .value
    }
}
