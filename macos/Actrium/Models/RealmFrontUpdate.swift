struct RealmFrontSelection: Equatable {
    let front: String
    let service: String
}

enum RealmFrontRefreshAction: Equatable {
    case keepRunning
    case loadAvailable(prepareService: Bool)

    init(
        running: RealmFrontSelection?,
        installed: RealmFrontSelection?,
        available: RealmFrontSelection,
        reloadLocalFront: Bool = false
    ) {
        if running == available && !reloadLocalFront {
            self = .keepRunning
        } else {
            let previous = running ?? installed
            self = .loadAvailable(prepareService: previous.map { $0 != available } ?? false)
        }
    }
}
