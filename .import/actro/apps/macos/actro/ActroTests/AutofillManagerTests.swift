import Foundation
import Testing
@testable import Actro

@MainActor
struct AutofillManagerTests {
    private let manager = AutofillManager.shared

    // MARK: - Domain Extraction

    @Test func extractDomain_validURL() {
        let domain = manager.extractDomain(from: URL(string: "https://accounts.google.com/login")!)
        #expect(domain == "accounts.google.com")
    }

    @Test func extractDomain_withPort() {
        let domain = manager.extractDomain(from: URL(string: "https://localhost:8080/page")!)
        #expect(domain == "localhost")
    }

    @Test func extractDomain_noHost_returnsNil() {
        let domain = manager.extractDomain(from: URL(string: "about:blank")!)
        #expect(domain == nil)
    }

    // MARK: - Dismiss & Toggle State Machine

    @Test func dismissBanner_movesPendingToHidden() {
        let action = SavePasswordAction(
            url: "https://example.com",
            domain: "example.com",
            username: "user",
            password: "pass",
            isUpdate: false,
            existingItem: nil
        )
        manager.pendingSaveAction = action
        #expect(manager.pendingSaveAction != nil)

        manager.dismissBanner()

        #expect(manager.pendingSaveAction == nil)
        #expect(manager.hiddenSaveAction != nil)
        #expect(manager.hiddenSaveAction?.username == "user")
    }

    @Test func toggleSaveDialog_hidesWhenVisible() {
        let action = SavePasswordAction(
            url: "https://example.com",
            domain: "example.com",
            username: "user",
            password: "pass",
            isUpdate: false,
            existingItem: nil
        )
        manager.pendingSaveAction = action

        manager.toggleSaveDialog()

        #expect(manager.pendingSaveAction == nil)
        #expect(manager.hiddenSaveAction != nil)
    }

    @Test func toggleSaveDialog_restoresWhenHidden() {
        let action = SavePasswordAction(
            url: "https://example.com",
            domain: "example.com",
            username: "user",
            password: "pass",
            isUpdate: false,
            existingItem: nil
        )
        manager.pendingSaveAction = action
        manager.dismissBanner() // pending → hidden

        manager.toggleSaveDialog() // hidden → pending

        #expect(manager.pendingSaveAction != nil)
        #expect(manager.pendingSaveAction?.username == "user")
        #expect(manager.hiddenSaveAction == nil)
    }

    @Test func toggleSaveDialog_noop_whenBothNil() {
        // Ensure clean state: set a dummy action and use neverSave to clear both
        manager.pendingSaveAction = SavePasswordAction(
            url: "https://cleanup.test",
            domain: "cleanup.test",
            username: "x",
            password: "x",
            isUpdate: false,
            existingItem: nil
        )
        manager.neverSaveForDomain() // clears both pending and hidden
        manager.neverSaveDomains.remove("cleanup.test")

        manager.toggleSaveDialog()

        #expect(manager.pendingSaveAction == nil)
    }

    // MARK: - Never Save Domain

    @Test func neverSaveForDomain_addsDomainToSet() {
        let action = SavePasswordAction(
            url: "https://spam.com/login",
            domain: "spam.com",
            username: "user",
            password: "pass",
            isUpdate: false,
            existingItem: nil
        )
        manager.pendingSaveAction = action

        manager.neverSaveForDomain()

        #expect(manager.neverSaveDomains.contains("spam.com"))
        #expect(manager.pendingSaveAction == nil)

        // Cleanup
        manager.neverSaveDomains.remove("spam.com")
    }
}
