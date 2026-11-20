import ActriumCore
import AppKit

/// Missing historical identity evidence can only be bridged by an explicit Host decision.
@MainActor
final class StorageMigrationConsent {
    typealias Presenter = @MainActor (String, NSWindow) async -> Bool
    private var decisions: [String: Bool] = [:]
    private var pending: [String: Task<Bool, Never>] = [:]
    private let present: Presenter

    init(present: Presenter? = nil) {
        self.present = present ?? Self.presentAlert
    }

    func resolve(
        origin: String, publisherIdentity: String?, database: GuestStorageDatabase, window: NSWindow?
    ) async throws -> String {
        do {
            return try await database.resolveSignedOrigin(origin, publisherIdentity: publisherIdentity)
        } catch GuestLocalStorageError.MigrationRequired {
            guard let window else { throw GuestLocalStorageError.MigrationRequired(message: "A Host window is required to review this storage copy.") }
            guard let publisherIdentity else { throw GuestLocalStorageError.MigrationRequired(message: "Verified Publisher context is required.") }
            guard let token = try await database.migrationToken(origin, publisherIdentity: publisherIdentity) else {
                return try await database.resolveSignedOrigin(origin, publisherIdentity: publisherIdentity)
            }
            let approved = await decision(origin: origin, token: token, window: window)
            try Task.checkCancellation()
            guard approved else { return try await database.keepSeparate(origin, publisherIdentity: publisherIdentity) }
            do {
                return try await database.resolveSignedOrigin(origin, publisherIdentity: publisherIdentity, migrationApproval: token)
            } catch let error as GuestLocalStorageError {
                let alert = NSAlert()
                alert.messageText = "Saved data was not copied"
                alert.informativeText = "The sources may have changed, contain conflicting values, or exceed the storage limit. The original data has been preserved. Reopen the application to review the copy."
                alert.addButton(withTitle: "OK")
                await alert.beginSheetModal(for: window)
                throw error
            }
        }
    }

    private func decision(origin: String, token: String, window: NSWindow) async -> Bool {
        let scope = token
        if let decision = decisions[scope] { return decision }
        if let task = pending[scope] { return await task.value }
        let task = Task { @MainActor in await present(origin, window) }
        pending[scope] = task
        let approved = await task.value
        pending.removeValue(forKey: scope)
        if !approved { decisions[scope] = false }
        return approved
    }

    private static func presentAlert(origin: String, window: NSWindow) async -> Bool {
        let alert = NSAlert()
        let program = origin.split(separator: ":").last.map(String.init) ?? "this application"
        alert.messageText = "Copy existing saved data to this Publisher?"
        alert.informativeText = "\(program) has saved data without verified Publisher history. Copy it only if you trust the current Publisher. The original data is kept. After this one-time copy, the original and Publisher storage stay separate and do not share later changes."
        alert.alertStyle = .warning
        alert.addButton(withTitle: "Keep Separate")
        alert.addButton(withTitle: "Copy Saved Data")
        return await alert.beginSheetModal(for: window) == .alertSecondButtonReturn
    }
}
