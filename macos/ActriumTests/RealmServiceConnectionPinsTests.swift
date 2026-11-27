import Foundation
import Testing
@testable import Actrium

struct RealmServiceConnectionPinsTests {
    @Test func sendsToTheOriginalInstanceAfterDiscoveryChanges() throws {
        let scope = UUID()
        var pins = RealmServiceConnectionPins<String>()
        let upload = try #require(pins.retain("service-A", scope: scope))
        let otherUpload = try #require(pins.retain("service-B", scope: scope))

        for _ in 0..<448 {
            #expect(pins.connection(id: upload, scope: scope) == "service-A")
        }
        #expect(pins.connection(id: otherUpload, scope: scope) == "service-B")
        pins.release(id: upload, scope: scope)
        #expect(pins.connection(id: upload, scope: scope) == nil)
    }

    @Test func aReloadCannotReuseOrReleaseThePreviousGenerationConnection() throws {
        let oldScope = UUID()
        let newScope = UUID()
        var pins = RealmServiceConnectionPins<String>()
        let id = try #require(pins.retain("service-A", scope: oldScope))

        #expect(pins.connection(id: id, scope: newScope) == nil)
        pins.release(id: id, scope: newScope)
        #expect(pins.connection(id: id, scope: oldScope) == "service-A")
        pins.retire(scope: oldScope)
        #expect(pins.connection(id: id, scope: oldScope) == nil)
    }

    @Test func anAbandonedBindingExpires() throws {
        let scope = UUID()
        let now = ContinuousClock.now
        var pins = RealmServiceConnectionPins<String>()
        let id = try #require(pins.retain("service-A", scope: scope, now: now))

        #expect(pins.connection(id: id, scope: scope, now: now.advanced(by: .seconds(299))) == "service-A")
        #expect(pins.connection(id: id, scope: scope, now: now.advanced(by: .seconds(300))) == nil)
    }
}
