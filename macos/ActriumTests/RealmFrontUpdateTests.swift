import Testing
@testable import Actrium

@MainActor
struct RealmFrontUpdateTests {
    private let original = RealmFrontSelection(front: "front-1", service: "service-1")

    @Test func aRunningTabRequiresApprovalForFrontServiceAndRollback() {
        let upgradedFront = RealmFrontSelection(front: "front-2", service: "service-1")
        let upgradedService = RealmFrontSelection(front: "front-1", service: "service-2")

        for (running, available) in [(original, upgradedFront), (original, upgradedService), (upgradedService, original)] {
            #expect(RealmFrontRefreshAction(running: running, installed: nil, hasInstalledProfile: true,
                                            available: available, approved: nil) == .offerUpdate)
            #expect(RealmFrontRefreshAction(running: running, installed: nil, hasInstalledProfile: true,
                                            available: available, approved: available) == .loadAvailable)
        }
        #expect(RealmFrontRefreshAction(running: original, installed: nil, hasInstalledProfile: true,
                                        available: upgradedService, approved: upgradedFront) == .offerUpdate)
        #expect(RealmFrontRefreshAction(running: original, installed: nil, hasInstalledProfile: true,
                                        available: original, approved: nil) == .keepRunning)
    }

    @Test func reopeningARealmKeepsTheInstalledSelectionUntilApproval() {
        let update = RealmFrontSelection(front: "front-2", service: "service-2")
        #expect(RealmFrontRefreshAction(running: nil, installed: original, hasInstalledProfile: true,
                                        available: update, approved: nil) == .offerUpdate)
        #expect(RealmFrontRefreshAction(running: nil, installed: nil, hasInstalledProfile: true,
                                        available: update, approved: nil) == .offerUpdate)
        #expect(RealmFrontRefreshAction(running: nil, installed: original, hasInstalledProfile: true,
                                        available: original, approved: nil) == .loadAvailable)
        #expect(RealmFrontRefreshAction(running: nil, installed: nil, hasInstalledProfile: false,
                                        available: update, approved: nil) == .loadAvailable)
    }
}
