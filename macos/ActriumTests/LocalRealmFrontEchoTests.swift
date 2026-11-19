import AppKit
import ActriumCore
import ActriumIPC
import Foundation
import XCTest
@testable import Actrium

final class LocalRealmFrontEchoTests: XCTestCase {
    @MainActor
    func testAuthorizedRebuildReloadsFromRealmProfile() async throws {
        let env = ProcessInfo.processInfo.environment
        guard let path = env["ACTRIUM_REALM_FRONT_ECHO_DIR"],
              let address = env["ACTRIUM_REALM_FRONT_ECHO_URI"] else {
            throw XCTSkip("Requires a published test Realm and independently running Echo Service.")
        }
        let directory = URL(fileURLWithPath: path, isDirectory: true)
        let selected = directory.appendingPathComponent("front.actr")
        let rebuilt = directory.appendingPathComponent("rebuilt.actr")
        var picks = 0
        let viewModel = BrowserViewModel(
            chooseLocalFrontFile: { panel in
                picks += 1
                XCTAssertTrue(panel.message.contains("Refresh can load new builds"))
                return selected
            },
            saveBrowserSession: { _, _ in }
        )
        defer {
            if let tab = viewModel.currentTab { RealmServiceBindings.shared.remove(tab.id) }
        }
        let initial = try XCTUnwrap(viewModel.currentTab?.id)
        viewModel.navigateCurrentTab(to: address)
        let first = try await waitForFront(in: viewModel, replacing: initial) {
            let invite = try String(contentsOf: directory.appendingPathComponent("viewer-invite"), encoding: .utf8)
            viewModel.confirmInviteRedeem(try XCTUnwrap(viewModel.pendingInviteRedeem), code: invite)
        }
        XCTAssertEqual(first.url?.absoluteString, address)
        try await echo("original", tabId: first.id)

        try Data(contentsOf: rebuilt).write(to: selected, options: .atomic)
        viewModel.reloadCurrentTab()
        let second = try await waitForFront(in: viewModel, replacing: first.id)
        XCTAssertEqual(second.url, first.url)
        XCTAssertNotEqual(second.appId, first.appId)
        XCTAssertTrue(second.unifiedScript?.contains("Echo Reloaded") == true)
        XCTAssertEqual(viewModel.tabs.count, 1)
        XCTAssertEqual(picks, 1)
        try await echo("rebuilt", tabId: second.id)
    }

    @MainActor
    func testRememberedOpenRealmJoinsAndRefreshesWithoutAnInvite() async throws {
        let env = ProcessInfo.processInfo.environment
        guard let address = env["ACTRIUM_OPEN_REALM_TEST_URI"],
              let front = env["ACTRIUM_OPEN_REALM_TEST_FRONT"] else {
            throw XCTSkip("Requires a published Open Realm, local Front and independently running Echo Service.")
        }
        let approval = ExpectedFrontApproval(front: front)
        let timer = Timer(timeInterval: 0.05, repeats: true) { _ in
            MainActor.assumeIsolated { approval.acceptIfShown() }
        }
        RunLoop.main.add(timer, forMode: .modalPanel)
        defer { timer.invalidate() }

        let suiteName = "ActriumTests.OpenRealm.\(UUID())"
        let preferences = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { preferences.removePersistentDomain(forName: suiteName) }
        let link = try ActriumCore.parseActrUri(
            uri: address, installationId: InstallationIdProvider.getOrCreate()
        )
        let realmId = try XCTUnwrap(link.realmId)
        preferences.set(["\(realmId)@\(link.domain)"], forKey: "actr.redeemedRealms")
        let viewModel = BrowserViewModel(realmPreferences: preferences, saveBrowserSession: { _, _ in })
        defer {
            if let tab = viewModel.currentTab { RealmServiceBindings.shared.remove(tab.id) }
        }
        let rejectInvite: () throws -> Void = {
            throw NSError(domain: "LocalRealmFrontEchoTests", code: 2, userInfo: [
                NSLocalizedDescriptionKey: "An Open Realm requested an invite."
            ])
        }
        let initial = try XCTUnwrap(viewModel.currentTab?.id)
        viewModel.navigateCurrentTab(to: address)
        let frontTab = try await waitForFront(in: viewModel, replacing: initial, onInvite: rejectInvite)
        XCTAssertEqual(frontTab.url?.absoluteString, address)
        XCTAssertEqual(approval.count, 1)
        XCTAssertNil(viewModel.pendingInviteRedeem)
        try await echo("invite-free", tabId: frontTab.id)

        viewModel.reloadCurrentTab()
        let refreshed = try await waitForFront(in: viewModel, replacing: frontTab.id, onInvite: rejectInvite)
        XCTAssertEqual(refreshed.url, frontTab.url)
        XCTAssertEqual(approval.count, 1)
        XCTAssertNil(viewModel.pendingInviteRedeem)
        try await echo("invite-free after refresh", tabId: refreshed.id)
    }

    @MainActor
    func testInviteOnlyRealmStillPromptsForAnInvite() async throws {
        guard let address = ProcessInfo.processInfo.environment["ACTRIUM_INVITE_ONLY_REALM_TEST_URI"] else {
            throw XCTSkip("Requires a published invite-only Realm.")
        }
        let suiteName = "ActriumTests.InviteOnlyRealm.\(UUID())"
        let preferences = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { preferences.removePersistentDomain(forName: suiteName) }
        let viewModel = BrowserViewModel(realmPreferences: preferences, saveBrowserSession: { _, _ in })
        viewModel.navigateCurrentTab(to: address)
        let deadline = ContinuousClock.now + .seconds(15)
        while ContinuousClock.now < deadline, viewModel.pendingInviteRedeem == nil {
            try await Task.sleep(for: .milliseconds(100))
        }
        XCTAssertEqual(viewModel.pendingInviteRedeem?.uri, address)
        XCTAssertNil(viewModel.currentTab?.appId)
        viewModel.cancelInviteRedeem(try XCTUnwrap(viewModel.pendingInviteRedeem))
    }

    @MainActor
    private func waitForFront(
        in viewModel: BrowserViewModel,
        replacing previous: UUID,
        onInvite: () throws -> Void = {}
    ) async throws -> Tab {
        let deadline = ContinuousClock.now + .seconds(25)
        while ContinuousClock.now < deadline {
            if let tab = viewModel.currentTab,
               tab.id != previous, tab.appId?.hasPrefix("actrium.local-front/") == true {
                return tab
            }
            if viewModel.pendingInviteRedeem != nil { try onInvite() }
            try await Task.sleep(for: .milliseconds(100))
        }
        throw NSError(domain: "LocalRealmFrontEchoTests", code: 1, userInfo: [
            NSLocalizedDescriptionKey: "The approved Front did not attach to the test Realm."
        ])
    }

    @MainActor
    private func echo(_ message: String, tabId: UUID) async throws {
        let body = Data(message.utf8)
        let response = try await RealmServiceBindings.shared.call(
            RealmServiceCall(
                id: UUID(), route: "echo.v1.EchoService.Echo",
                payload: Data([10, UInt8(body.count)]) + body, timeoutMs: 15000
            ),
            tabId: tabId
        )
        let expected = Data("echo: \(message)".utf8)
        XCTAssertEqual(response, Data([10, UInt8(expected.count)]) + expected)
    }
}

@MainActor
private final class ExpectedFrontApproval {
    private let front: String
    private(set) var count = 0

    init(front: String) {
        self.front = front
    }

    // Only act on the local Front confirmation in this XCTest process.
    func acceptIfShown() {
        guard count == 0, let view = NSApp.modalWindow?.contentView else { return }
        let views = descendants(of: view)
        let captions = views.compactMap { ($0 as? NSTextField)?.stringValue ?? ($0 as? NSTextView)?.string }
        guard captions.contains("Open this local Realm Front?"),
              captions.contains(where: { $0.contains(front) }),
              let button = views.compactMap({ $0 as? NSButton }).first(where: { $0.title == "Allow and Open" }) else { return }
        count += 1
        button.performClick(nil)
    }

    private func descendants(of view: NSView) -> [NSView] {
        [view] + view.subviews.flatMap { descendants(of: $0) }
    }
}
