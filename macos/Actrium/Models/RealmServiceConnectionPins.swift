import Foundation

/// An upload may only use the connection selected by its initial RPC.
nonisolated struct RealmServiceConnectionPins<Connection: Sendable>: Sendable {
    private struct Pin: Sendable {
        let scope: UUID
        let connection: Connection
        let deadline: ContinuousClock.Instant
    }

    private var pins: [UUID: Pin] = [:]

    mutating func retain(
        _ connection: Connection, scope: UUID, now: ContinuousClock.Instant = .now
    ) -> UUID? {
        pins = pins.filter { $0.value.deadline > now }
        guard pins.count < 32 else { return nil }
        let id = UUID()
        pins[id] = Pin(scope: scope, connection: connection, deadline: now.advanced(by: .seconds(300)))
        return id
    }

    mutating func connection(
        id: UUID, scope: UUID, now: ContinuousClock.Instant = .now
    ) -> Connection? {
        guard let pin = pins[id], pin.scope == scope else { return nil }
        guard pin.deadline > now else {
            pins.removeValue(forKey: id)
            return nil
        }
        return pin.connection
    }

    mutating func release(id: UUID, scope: UUID) {
        if pins[id]?.scope == scope { pins.removeValue(forKey: id) }
    }

    mutating func retire(scope: UUID) {
        pins = pins.filter { $0.value.scope != scope }
    }
}
