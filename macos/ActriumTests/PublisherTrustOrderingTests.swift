import ActriumCore
import CryptoKit
import Foundation
import Testing
@testable import Actrium

@MainActor
struct PublisherTrustOrderingTests {
    @Test func revocationCannotCommitBetweenTheDatabaseReadAndFrontCommit() async throws {
        let database = GuestStorageDatabase(database: GuestLocalStorage(path: ":memory:"))
        _ = try await database.acceptPublisherProfile(body: PublisherStorageFixture.profile(rotated: false), domain: "publisher.example")
        let read = AsyncStream<Void>.makeStream()
        let resume = AsyncStream<Void>.makeStream()
        var order: [String] = []
        let activation = Task { @MainActor in
            try await database.withPublisherTrust {
                let snapshot = try await database.acceptedPublishers(domains: ["publisher.example"])
                #expect(snapshot.first?.packageSigningKeys.first?.publicKeyBase64 == (try PublisherStorageFixture.key(3).publicKey.rawRepresentation.base64EncodedString()))
                order.append("read")
                read.continuation.yield(())
                for await _ in resume.stream { break }
                order.append("switch")
            }
        }
        defer { resume.continuation.finish() }
        var reads = read.stream.makeAsyncIterator()
        _ = await reads.next()
        let revoke = Task { @MainActor in
            _ = try await database.acceptPublisherProfile(body: PublisherStorageFixture.profile(rotated: true), domain: "publisher.example")
            order.append("revoke")
        }
        let deadline = ContinuousClock.now + .seconds(5)
        while database.pendingPublisherTrustCount < 2 && ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(1))
        }
        #expect(database.pendingPublisherTrustCount == 2)
        #expect(order == ["read"])
        resume.continuation.yield(())
        resume.continuation.finish()
        try await activation.value
        try await revoke.value
        #expect(order == ["read", "switch", "revoke"])
        do {
            try await RealmProfileStore.withPublisherKeys([
                .init(publicKey: PublisherStorageFixture.key(3).publicKey.rawRepresentation, domain: "publisher.example")
            ], database: database) { order.append("invalid switch") }
            Issue.record("Revocation did not prevent a later switch.")
        } catch is RealmServiceError { }
        #expect(!order.contains("invalid switch"))
    }

    @Test func failedActivationReleasesTheTrustOrderingGate() async throws {
        let database = GuestStorageDatabase(database: GuestLocalStorage(path: ":memory:"))
        _ = try await database.acceptPublisherProfile(body: PublisherStorageFixture.profile(rotated: false), domain: "publisher.example")
        enum Aborted: Error { case activation }
        do {
            try await RealmProfileStore.withPublisherKeys([
                .init(publicKey: PublisherStorageFixture.key(3).publicKey.rawRepresentation, domain: "publisher.example")
            ], database: database) { throw Aborted.activation }
        } catch Aborted.activation { }
        #expect(database.pendingPublisherTrustCount == 0)
        _ = try await database.acceptPublisherProfile(body: PublisherStorageFixture.profile(rotated: true), domain: "publisher.example")
        #expect(database.pendingPublisherTrustCount == 0)
    }

    @Test func aCancelledWaiterDoesNotBlockLaterPublisherUpdates() async throws {
        let database = GuestStorageDatabase(database: GuestLocalStorage(path: ":memory:"))
        let started = AsyncStream<Void>.makeStream()
        let resume = AsyncStream<Void>.makeStream()
        let active = Task { @MainActor in
            try await database.withPublisherTrust {
                started.continuation.yield(())
                for await _ in resume.stream { break }
            }
        }
        defer { resume.continuation.finish() }
        var events = started.stream.makeAsyncIterator()
        _ = await events.next()
        let cancelled = Task { @MainActor in
            try await database.withPublisherTrust { _ = Issue.record("Cancelled operation entered its commit.") }
        }
        let deadline = ContinuousClock.now + .seconds(5)
        while database.pendingPublisherTrustCount < 2 && ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(1))
        }
        try #require(database.pendingPublisherTrustCount == 2)
        cancelled.cancel()
        resume.continuation.yield(())
        resume.continuation.finish()
        try await active.value
        do { try await cancelled.value; Issue.record("The waiter was not cancelled.") }
        catch is CancellationError { }
        _ = try await database.acceptPublisherProfile(body: PublisherStorageFixture.profile(rotated: false), domain: "publisher.example")
        #expect(database.pendingPublisherTrustCount == 0)
    }
}
