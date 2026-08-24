import Foundation
import Combine

struct SavePasswordAction {
    let url: String
    let domain: String
    let username: String
    let password: String
    let isUpdate: Bool
    let existingItem: VaultItem?
}

/// Password autofill is origin-bound. Domain suffix matching can leak a parent
/// site's credentials into an unrelated subdomain (or the reverse), while
/// ignoring the scheme can leak an HTTPS password onto an HTTP page.
nonisolated struct CredentialOrigin: Equatable {
    let scheme: String
    let host: String
    let port: Int

    init?(url: URL) {
        guard let scheme = url.scheme?.lowercased(),
              scheme == "http" || scheme == "https",
              let rawHost = url.host?.lowercased(),
              !rawHost.isEmpty else { return nil }
        self.scheme = scheme
        self.host = rawHost.hasSuffix(".") ? String(rawHost.dropLast()) : rawHost
        self.port = url.port ?? (scheme == "https" ? 443 : 80)
    }

    var storageURLString: String {
        var components = URLComponents()
        components.scheme = scheme
        components.host = host
        let defaultPort = scheme == "https" ? 443 : 80
        components.port = port == defaultPort ? nil : port
        return components.string ?? "\(scheme)://\(host)"
    }
}

@MainActor
final class AutofillManager: ObservableObject {
    static let shared = AutofillManager()

    @Published var pendingSaveAction: SavePasswordAction?
    /// Hidden save action (dismissed by user but restorable via key icon)
    private(set) var hiddenSaveAction: SavePasswordAction?

    var neverSaveDomains: Set<String> {
        didSet {
            UserDefaults.standard.set(Array(neverSaveDomains), forKey: "autofill.neverSaveDomains")
        }
    }

    private init() {
        let stored = UserDefaults.standard.stringArray(forKey: "autofill.neverSaveDomains") ?? []
        self.neverSaveDomains = Set(stored)
    }

    // MARK: - Domain Extraction

    func extractDomain(from url: URL) -> String? {
        url.host
    }

    // MARK: - Credential Lookup

    /// Find login credentials matching the page's exact origin.
    /// Returns tuples of (VaultItem, username, password) for matching items.
    func findCredentials(for pageURL: URL) -> [(VaultItem, String, String)] {
        guard let pageOrigin = CredentialOrigin(url: pageURL) else { return [] }
        let vaultItems = VaultManager.shared.items.filter { $0.category == .login }

        return vaultItems.compactMap { item in
            guard let urlFieldValue = item.fields.first(where: { $0.label == "URL" })?.value,
                  !urlFieldValue.isEmpty,
                  urlFieldValue.utf8.count <= 65_536 else {
                return nil
            }
            // Handle URLs without scheme
            let normalized = urlFieldValue.hasPrefix("http") ? urlFieldValue : "https://\(urlFieldValue)"
            guard let itemURL = URL(string: normalized),
                  let itemOrigin = CredentialOrigin(url: itemURL) else {
                return nil
            }

            guard itemOrigin == pageOrigin else { return nil }

            let username = item.fields.first(where: { $0.label == "Username" })?.value ?? ""
            let password = item.fields.first(where: { $0.label == "Password" })?.value ?? ""
            return (item, username, password)
        }
    }

    // MARK: - Handle Captured Credentials

    func handleCapturedCredentials(url: String, username: String, password: String) {
        // Allow capture regardless of vault state — the save banner should still
        // appear so the user knows credentials were detected. Saving will require
        // vault unlock, but the prompt itself should not be gated.
        guard VaultManager.shared.state != .notSetup else { return }

        guard url.utf8.count <= 65_536,
              username.utf8.count <= 4_096,
              password.utf8.count <= 16_384,
              let parsedURL = URL(string: url),
              let origin = CredentialOrigin(url: parsedURL) else { return }
        let domain = origin.host
        let storageURL = origin.storageURLString

        // Check never-save list
        guard !neverSaveDomains.contains(domain) else { return }

        // Check existing credentials
        let existing = findCredentials(for: parsedURL)

        // Check if we already have an entry with the same username
        if let match = existing.first(where: { $0.1 == username }) {
            // Same password already saved — nothing to do
            if match.2 == password { return }

            // Different password — offer to update
            pendingSaveAction = SavePasswordAction(
                url: storageURL,
                domain: domain,
                username: username,
                password: password,
                isUpdate: true,
                existingItem: match.0
            )
        } else {
            // New credentials — offer to save
            pendingSaveAction = SavePasswordAction(
                url: storageURL,
                domain: domain,
                username: username,
                password: password,
                isUpdate: false,
                existingItem: nil
            )
        }
    }

    // MARK: - Save / Update / Dismiss

    func saveCredentials() {
        guard let action = pendingSaveAction else { return }

        let fields: [VaultField] = [
            VaultField(label: "URL", value: action.url, isSensitive: false),
            VaultField(label: "Username", value: action.username, isSensitive: false),
            VaultField(label: "Password", value: action.password, isSensitive: true),
        ]

        let item = VaultItem(
            id: UUID().uuidString,
            title: action.domain,
            category: .login,
            fields: fields,
            notes: nil,
            createdAt: Date(),
            updatedAt: Date()
        )

        try? VaultManager.shared.addItem(item)
        pendingSaveAction = nil
        hiddenSaveAction = nil
    }

    func updateCredentials() {
        guard let action = pendingSaveAction,
              let existingItem = action.existingItem else { return }

        var updatedFields = existingItem.fields
        if let index = updatedFields.firstIndex(where: { $0.label == "Password" }) {
            updatedFields[index].value = action.password
        }

        var updatedItem = existingItem
        updatedItem.fields = updatedFields

        try? VaultManager.shared.updateItem(updatedItem)
        pendingSaveAction = nil
        hiddenSaveAction = nil
    }

    func neverSaveForDomain() {
        guard let action = pendingSaveAction else { return }
        neverSaveDomains.insert(action.domain)
        pendingSaveAction = nil
        hiddenSaveAction = nil
    }

    func dismissBanner() {
        hiddenSaveAction = pendingSaveAction
        pendingSaveAction = nil
    }

    /// Toggle the save dialog visibility (for key icon tap)
    func toggleSaveDialog() {
        if pendingSaveAction != nil {
            dismissBanner()
        } else if let hidden = hiddenSaveAction {
            pendingSaveAction = hidden
            hiddenSaveAction = nil
        }
    }
}
