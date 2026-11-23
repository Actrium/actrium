import Testing
@testable import Actrium

@MainActor
struct RealmFrontUpdateTests {
    private let original = RealmFrontSelection(front: "front-1", service: "service-1")

    @Test func changedSelectionsPrepareServiceBeforeReplacingARunningFront() {
        let upgradedFront = RealmFrontSelection(front: "front-2", service: "service-1")
        let upgradedService = RealmFrontSelection(front: "front-1", service: "service-2")

        for (running, available) in [(original, upgradedFront), (original, upgradedService), (upgradedService, original)] {
            let action = RealmFrontRefreshAction(running: running, installed: nil, available: available)
            #expect(action == .loadAvailable(prepareService: true))
        }
    }

    @Test func unchangedSelectionKeepsRunning() {
        let action = RealmFrontRefreshAction(running: original, installed: nil, available: original)
        #expect(action == .keepRunning)
    }

    @Test func manualReloadCanReadAnApprovedLocalFrontAgain() {
        let action = RealmFrontRefreshAction(
            running: original, installed: nil, available: original, reloadLocalFront: true
        )
        #expect(action == .loadAvailable(prepareService: false))
    }

    @Test func reopeningAppliesAChangeToTheInstalledSelection() {
        let update = RealmFrontSelection(front: "front-2", service: "service-2")
        let action = RealmFrontRefreshAction(running: nil, installed: original, available: update)
        #expect(action == .loadAvailable(prepareService: true))
    }

    @Test func openingWithNoSelectionChangeLoadsTheAvailableFront() {
        for installed in [nil, original] {
            let action = RealmFrontRefreshAction(running: nil, installed: installed, available: original)
            #expect(action == .loadAvailable(prepareService: false))
        }
    }
}
