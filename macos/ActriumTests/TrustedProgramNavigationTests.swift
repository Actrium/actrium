import Foundation
import Testing
@testable import Actrium

@MainActor
@Suite(.serialized)
struct TrustedProgramNavigationTests {
    private func program() -> VerifiedLocalProgram {
        VerifiedLocalProgram(publisher: "publisher.example.test", program: "echo", version: "1.0.0",
                            digest: String(repeating: "a", count: 64), signingKeyId: "test",
                            title: "Verified Program", description: nil, guestCode: "globalThis.verified = true;",
                            serviceBytes: [0], resources: [], metadataJson: "{}")
    }
    private func settle(_ condition: () -> Bool) async throws {
        let deadline = ContinuousClock.now.advanced(by: .seconds(3))
        while !condition(), ContinuousClock.now < deadline { try await Task.sleep(for: .milliseconds(10)) }
        #expect(condition())
    }
    @Test func explicitHTTPSPackageUsesVerifiedBytesAndDoesNotGrantRequestedCapabilities() async throws {
        let vm = BrowserViewModel(persistSession: {})
        let verified = program()
        var requested: TrustedProgramSource?
        vm.loadTrustedProgram = { source in requested = source; return verified }
        vm.navigateCurrentTab(to: "https://cdn.example.test/program.actr")
        try await settle { vm.currentTab?.unifiedScript != nil }
        #expect(requested?.href == "https://cdn.example.test/program.actr")
        #expect(requested?.expectedSha256 == nil)
        #expect(vm.currentTab?.unifiedScript == verified.guestCode)
        #expect(vm.currentTab?.verifiedProgram?.serviceBytes == verified.serviceBytes)
        #expect(vm.currentTab?.permissions.isEmpty == true)
        #expect(vm.currentTab?.url?.absoluteString == "https://cdn.example.test/program.actr")
    }
    @Test(arguments: [false, true])
    func staleSuccessAndFailureCannotOverwriteLaterNavigation(fail: Bool) async throws {
        let vm = BrowserViewModel(persistSession: {})
        var pending: CheckedContinuation<VerifiedLocalProgram, Error>?
        vm.loadTrustedProgram = { _ in try await withCheckedThrowingContinuation { pending = $0 } }
        vm.navigateCurrentTab(to: "https://cdn.example.test/program.actr")
        try await settle { pending != nil }
        vm.navigateCurrentTab(to: "https://example.test/next")
        if fail { pending?.resume(throwing: URLError(.cannotDecodeContentData)) }
        else { pending?.resume(returning: program()) }
        try await Task.sleep(for: .milliseconds(30))
        #expect(vm.currentTab?.url?.absoluteString == "https://example.test/next")
        #expect(vm.currentTab?.unifiedScript == nil)
        #expect(vm.currentTab?.title != "Unable to Verify Program")
    }
    @Test func closingTabCancelsDeliveryToThatTab() async throws {
        let vm = BrowserViewModel(persistSession: {})
        let other = vm.currentTabId
        let target = vm.addNewTab().id
        var pending: CheckedContinuation<VerifiedLocalProgram, Error>?
        vm.loadTrustedProgram = { _ in try await withCheckedThrowingContinuation { pending = $0 } }
        vm.navigateCurrentTab(to: "https://cdn.example.test/program.actr")
        try await settle { pending != nil }
        vm.closeTab(id: target)
        pending?.resume(returning: program())
        try await Task.sleep(for: .milliseconds(30))
        #expect(vm.tabs.allSatisfy { $0.id != target })
        #expect(vm.tabs.first(where: { $0.id == other })?.unifiedScript == nil)
    }
    @Test func acceptedRealmAdapterPreservesBothExpectedIdentityAndDigest() async throws {
        let vm = BrowserViewModel(persistSession: {})
        var selection: TrustedProgramSource?
        let verified = program()
        vm.loadTrustedProgram = { source in selection = source; return verified }
        vm.openAcceptedRealmPackage(href: "https://cdn.example.test/program.actr", sha256: verified.digest,
                                    actorType: "publisher.example.test/echo@1.0.0",
                                    realmURL: URL(string: "actr://realm.example.test/k7m2q9xh")!)
        try await settle { selection != nil && vm.currentTab?.unifiedScript != nil }
        #expect(selection?.expectedSha256 == verified.digest)
        #expect(selection?.expectedActorType == "publisher.example.test/echo@1.0.0")
        #expect(vm.currentTab?.url?.scheme == "actr")
    }
}
