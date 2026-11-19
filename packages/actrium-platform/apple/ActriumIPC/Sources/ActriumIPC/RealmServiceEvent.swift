import Foundation

/// A renderer-owned subscription to the Service selected for one Realm tab.
public struct RealmServiceEventSubscription: Codable, Sendable {
    public static let requestEvent = "com.actrium.realmService.events.subscribe"
    public static let responseEvent = "com.actrium.realmService.events.subscribed"
    public static let cancelEvent = "com.actrium.realmService.events.unsubscribe"
    public static let eventEvent = "com.actrium.realmService.events.event"

    public let id: UUID
    public let events: [String]

    public init(id: UUID, events: [String]) {
        self.id = id
        self.events = events
    }
}

public struct RealmServiceEvent: Codable, Sendable {
    public let subscriptionId: UUID
    public let event: String
    public let payload: Data

    public init(subscriptionId: UUID, event: String, payload: Data) {
        self.subscriptionId = subscriptionId
        self.event = event
        self.payload = payload
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
