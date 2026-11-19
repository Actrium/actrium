import Foundation
import Testing
@testable import Actrium

@MainActor
struct RealmFrontUpdateTests {
    private let original = RealmFrontSelection(front: "front-1", service: "service-1")

    @Test func aRunningTabRequiresApprovalForFrontServiceAndRollback() {
        let upgradedFront = RealmFrontSelection(front: "front-2", service: "service-1")
        let upgradedService = RealmFrontSelection(front: "front-1", service: "service-2")

        for (running, available) in [(original, upgradedFront), (original, upgradedService), (upgradedService, original)] {
            #expect(RealmFrontRefreshAction(running: running, installed: nil,
                                            available: available, approved: nil) == .offerUpdate(from: running))
            #expect(RealmFrontRefreshAction(running: running, installed: nil,
                                            available: available, approved: available) == .loadAvailable)
        }
        #expect(RealmFrontRefreshAction(running: original, installed: nil,
                                        available: upgradedService, approved: upgradedFront) == .offerUpdate(from: original))
        #expect(RealmFrontRefreshAction(running: original, installed: nil,
                                        available: original, approved: nil) == .keepRunning)
    }

    @Test func reopeningARealmKeepsTheInstalledSelectionUntilApproval() {
        let update = RealmFrontSelection(front: "front-2", service: "service-2")
        #expect(RealmFrontRefreshAction(running: nil, installed: original,
                                        available: update, approved: nil) == .offerUpdate(from: original))
        #expect(RealmFrontRefreshAction(running: nil, installed: original,
                                        available: original, approved: nil) == .loadAvailable)
    }

    @Test func reopeningWithoutAVerifiableInstalledProfileLoadsAvailable() {
        let update = RealmFrontSelection(front: "front-2", service: "service-2")
        #expect(RealmFrontRefreshAction(running: nil, installed: nil,
                                        available: update, approved: nil) == .loadAvailable)
    }

    @Test func noticeNamesTheSelectedPackageChanges() {
        let tabId = UUID()
        let address = URL(string: "actr://realm.example/echo")!
        let candidates = [
            (RealmFrontSelection(front: "front-2", service: "service-1"), String(localized: "Realm Front update available")),
            (RealmFrontSelection(front: "front-1", service: "service-2"), String(localized: "Realm Service update available")),
            (RealmFrontSelection(front: "front-2", service: "service-2"), String(localized: "Realm Front and Service update available"))
        ]
        for (selection, title) in candidates {
            let offer = RealmFrontUpdateOffer(tabId: tabId, address: address, navigationNonce: 1,
                                              previous: original, selection: selection)
            #expect(offer.title == title)
        }
    }
}
