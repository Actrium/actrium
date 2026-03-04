import Foundation

struct VaultItem: Identifiable, Codable {
    let id: String
    var title: String
    var category: VaultItemCategory
    var fields: [VaultField]
    var notes: String?
    var createdAt: Date
    var updatedAt: Date

    /// First non-sensitive field value, used as subtitle in list
    var subtitle: String? {
        fields.first(where: { !$0.isSensitive && !$0.value.isEmpty })?.value
    }

    /// First sensitive field value, used for quick copy
    var primarySensitiveValue: String? {
        fields.first(where: { $0.isSensitive && !$0.value.isEmpty })?.value
    }

    // MARK: - Login Convenience Properties

    /// Domain extracted from the URL field (login items only)
    var loginDomain: String? {
        guard category == .login,
              let urlString = fields.first(where: { $0.label == "URL" })?.value,
              !urlString.isEmpty else { return nil }
        // Try parsing as URL, add scheme if missing
        let normalized = urlString.hasPrefix("http") ? urlString : "https://\(urlString)"
        return URL(string: normalized)?.host
    }

    /// Username field value (login items only)
    var loginUsername: String? {
        guard category == .login else { return nil }
        return fields.first(where: { $0.label == "Username" })?.value
    }

    /// Password field value (login items only)
    var loginPassword: String? {
        guard category == .login else { return nil }
        return fields.first(where: { $0.label == "Password" })?.value
    }
}

enum VaultItemCategory: String, Codable, CaseIterable {
    case login
    case sshKey
    case apiKey
    case secureNote

    var icon: String {
        switch self {
        case .login: return "key.fill"
        case .sshKey: return "terminal.fill"
        case .apiKey: return "bolt.shield.fill"
        case .secureNote: return "note.text"
        }
    }

    var label: String {
        switch self {
        case .login: return "Login"
        case .sshKey: return "SSH Key"
        case .apiKey: return "API Key"
        case .secureNote: return "Secure Note"
        }
    }

    var template: [VaultField] {
        switch self {
        case .login:
            return [
                VaultField(label: "URL", value: "", isSensitive: false),
                VaultField(label: "Username", value: "", isSensitive: false),
                VaultField(label: "Password", value: "", isSensitive: true),
            ]
        case .sshKey:
            return [
                VaultField(label: "Host", value: "", isSensitive: false),
                VaultField(label: "Port", value: "", isSensitive: false),
                VaultField(label: "Username", value: "", isSensitive: false),
                VaultField(label: "Private Key", value: "", isSensitive: true),
            ]
        case .apiKey:
            return [
                VaultField(label: "Service", value: "", isSensitive: false),
                VaultField(label: "API Key", value: "", isSensitive: true),
                VaultField(label: "API Secret", value: "", isSensitive: true),
            ]
        case .secureNote:
            return [
                VaultField(label: "Content", value: "", isSensitive: true),
            ]
        }
    }
}

struct VaultField: Codable, Identifiable {
    let id: UUID
    var label: String
    var value: String
    var isSensitive: Bool

    init(id: UUID = UUID(), label: String, value: String, isSensitive: Bool) {
        self.id = id
        self.label = label
        self.value = value
        self.isSensitive = isSensitive
    }
}
