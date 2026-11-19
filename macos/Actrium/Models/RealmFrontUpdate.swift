import Foundation

struct RealmFrontSelection: Equatable {
    let front: String
    let service: String
}

struct RealmFrontUpdateOffer: Equatable {
    let tabId: UUID
    let address: URL
    let navigationNonce: Int
    let selection: RealmFrontSelection
}

enum RealmFrontRefreshAction: Equatable {
    case keepRunning
    case offerUpdate
    case loadAvailable

    init(
        running: RealmFrontSelection?,
        installed: RealmFrontSelection?,
        hasInstalledProfile: Bool,
        available: RealmFrontSelection,
        approved: RealmFrontSelection?
    ) {
        if running == available {
            self = .keepRunning
        } else if approved == available || (running == nil && (!hasInstalledProfile || installed == available)) {
            self = .loadAvailable
        } else {
            self = .offerUpdate
        }
    }
}
