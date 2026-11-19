import Foundation

struct RealmFrontSelection: Equatable {
    let front: String
    let service: String
}

struct RealmFrontUpdateOffer: Equatable {
    let tabId: UUID
    let address: URL
    let navigationNonce: Int
    let previous: RealmFrontSelection
    let selection: RealmFrontSelection

    var title: String {
        if previous.service == selection.service { return String(localized: "Realm Front update available") }
        if previous.front == selection.front { return String(localized: "Realm Service update available") }
        return String(localized: "Realm Front and Service update available")
    }
}

enum RealmFrontRefreshAction: Equatable {
    case keepRunning
    case offerUpdate(from: RealmFrontSelection)
    case loadAvailable

    init(
        running: RealmFrontSelection?,
        installed: RealmFrontSelection?,
        available: RealmFrontSelection,
        approved: RealmFrontSelection?
    ) {
        if running == available {
            self = .keepRunning
        } else if approved != available, let previous = running ?? installed, previous != available {
            self = .offerUpdate(from: previous)
        } else {
            self = .loadAvailable
        }
    }
}
