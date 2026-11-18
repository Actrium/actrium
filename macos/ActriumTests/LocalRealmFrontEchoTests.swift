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
        if viewModel.pendingInviteRedeem != nil {
            let invite = try String(contentsOf: directory.appendingPathComponent("viewer-invite"), encoding: .utf8)
            viewModel.confirmInviteRedeem(code: invite)
        }
        let first = try await waitForFront(in: viewModel, replacing: initial)
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
    private func waitForFront(in viewModel: BrowserViewModel, replacing previous: UUID) async throws -> Tab {
        let deadline = ContinuousClock.now + .seconds(25)
        while ContinuousClock.now < deadline {
            if let tab = viewModel.currentTab,
               tab.id != previous, tab.appId?.hasPrefix("actrium.local-front/") == true {
                return tab
            }
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
