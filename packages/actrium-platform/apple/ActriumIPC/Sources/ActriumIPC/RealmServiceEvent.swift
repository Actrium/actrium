import Foundation

/// A renderer-owned subscription to the Service selected for one Realm tab.
public struct RealmServiceEventSubscription: Codable, Sendable {
    public static let requestEvent = "com.actrium.realmService.events.subscribe"
    public static let responseEvent = "com.actrium.realmService.events.subscribed"
    public static let cancelEvent = "com.actrium.realmService.events.unsubscribe"
    public static let eventEvent = "com.actrium.realmService.events.event"

    public let id: UUID
    public let events: [String]
    public let streamId: String?

    public init(id: UUID, events: [String] = [], streamId: String? = nil) {
        self.id = id
        self.events = events
        self.streamId = streamId
    }
}

public struct RealmServiceEvent: Codable, Sendable {
    public let subscriptionId: UUID
    public let event: String
    public let payload: Data
    public let sequence: UInt64?

    public init(subscriptionId: UUID, event: String, payload: Data, sequence: UInt64? = nil) {
        self.subscriptionId = subscriptionId
        self.event = event
        self.payload = payload
        self.sequence = sequence
    }
}

public struct RealmServiceEventReply: Codable, Sendable {
    public let id: UUID
    public let failure: RealmServiceFailure?

    public init(id: UUID, failure: RealmServiceFailure? = nil) {
        self.id = id
        self.failure = failure
    }
}
