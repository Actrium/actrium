import ActriumIPC
import Foundation
import Testing
@testable import Actrium

private actor ControlledRealmServiceBinding: RealmServiceBindingHandle {
    let id = UUID()
    private var subscribeContinuation: CheckedContinuation<Void, Error>?
    private var subscribeGeneration: UInt64 = 0
    private var completesSubscribeImmediately = false
    private(set) var suspendCount = 0

    func call(_ request: RealmServiceCall) async throws -> Data {
        Data()
    }

    func setCompletesSubscribeImmediately() {
        completesSubscribeImmediately = true
    }

    func subscribe(events: [String], generation: UInt64) async throws {
        subscribeGeneration = max(subscribeGeneration, generation)
        if completesSubscribeImmediately { return }
        try await withCheckedThrowingContinuation { continuation in
            subscribeContinuation = continuation
        }
    }

    func waitUntilSubscribeStarts() async {
        while subscribeContinuation == nil {
            await Task.yield()
        }
    }

    func completeSubscribe(with result: Result<Void, Error>) {
        let continuation = subscribeContinuation
        subscribeContinuation = nil
        continuation?.resume(with: result)
    }

    func suspendEvents(ifGeneration generation: UInt64) {
        guard generation == subscribeGeneration else { return }
        suspendCount += 1
    }

    func stop() {}
}

@MainActor
struct RealmServiceBindingLifecycleTests {
    @Test func publishesWhileSubscribeReplyIsPending() async throws {
        let bindings = RealmServiceBindings()
        let binding = ControlledRealmServiceBinding()
        let tabId = UUID()
        let subscriptionId = UUID()
        var delivered: [UUID] = []
        bindings.attach(binding, to: tabId)
        bindings.eventHandler = { receivedTab, event in
            #expect(receivedTab == tabId)
            #expect(event.event == "notes.changed")
            delivered.append(event.subscriptionId)
        }
        let request = RealmServiceEventSubscription(
            id: subscriptionId, events: ["notes.changed"]
        )
        let subscribe = Task {
            try await bindings.subscribe(request, tabId: tabId)
        }
        await binding.waitUntilSubscribeStarts()

        bindings.publish(bindingId: binding.id, event: "notes.changed", payload: Data([1, 2, 3]))
        for _ in 0..<20 where delivered.isEmpty {
            await Task.yield()
        }
        #expect(delivered == [subscriptionId])

        await binding.completeSubscribe(with: .success(()))
        try await subscribe.value
        bindings.unsubscribe(subscriptionId, tabId: tabId)
        for _ in 0..<20 {
            if await binding.suspendCount == 1 { break }
            await Task.yield()
        }
        let suspendCount = await binding.suspendCount
        #expect(suspendCount == 1)
    }

    @Test func pendingUnsubscribeRollsBackEarlyRegistration() async throws {
        let bindings = RealmServiceBindings()
        let binding = ControlledRealmServiceBinding()
        let tabId = UUID()
        let subscriptionId = UUID()
        var delivered: [UUID] = []
        bindings.attach(binding, to: tabId)
        bindings.eventHandler = { _, event in delivered.append(event.subscriptionId) }
        let request = RealmServiceEventSubscription(
            id: subscriptionId, events: ["notes.changed"]
        )
        let subscribe = Task {
            try await bindings.subscribe(request, tabId: tabId)
        }
        await binding.waitUntilSubscribeStarts()

        bindings.unsubscribe(subscriptionId, tabId: tabId)
        await binding.completeSubscribe(with: .success(()))
        do {
            try await subscribe.value
            Issue.record("A cancelled subscription must not complete successfully.")
        } catch is CancellationError {
            // Expected.
        }

        bindings.publish(bindingId: binding.id, event: "notes.changed", payload: Data())
        for _ in 0..<5 { await Task.yield() }
        #expect(delivered.isEmpty)
        let suspendCount = await binding.suspendCount
        #expect(suspendCount == 1)
    }

    @Test func immediateResubscribeKeepsTheNewEventPeer() async throws {
        let bindings = RealmServiceBindings()
        let binding = ControlledRealmServiceBinding()
        let tabId = UUID()
        let firstId = UUID()
        let secondId = UUID()
        var delivered: [UUID] = []
        bindings.attach(binding, to: tabId)
        bindings.eventHandler = { _, event in delivered.append(event.subscriptionId) }
        await binding.setCompletesSubscribeImmediately()

        try await bindings.subscribe(
            RealmServiceEventSubscription(id: firstId, events: ["notes.changed"]),
            tabId: tabId
        )
        bindings.unsubscribe(firstId, tabId: tabId)
        try await bindings.subscribe(
            RealmServiceEventSubscription(id: secondId, events: ["notes.changed"]),
            tabId: tabId
        )

        bindings.publish(
            bindingId: binding.id, event: "notes.changed", payload: Data([1])
        )
        for _ in 0..<20 where delivered.isEmpty {
            await Task.yield()
        }
        #expect(delivered == [secondId])
        let suspendCount = await binding.suspendCount
        #expect(suspendCount == 0)
        bindings.remove(tabId)
    }

    @Test func failedSubscribeDoesNotLeaveARoutableSubscription() async throws {
        let bindings = RealmServiceBindings()
        let binding = ControlledRealmServiceBinding()
        let tabId = UUID()
        let subscriptionId = UUID()
        var delivered: [UUID] = []
        bindings.attach(binding, to: tabId)
        bindings.eventHandler = { _, event in delivered.append(event.subscriptionId) }
        let request = RealmServiceEventSubscription(
            id: subscriptionId, events: ["notes.changed"]
        )
        let subscribe = Task {
            try await bindings.subscribe(request, tabId: tabId)
        }
        await binding.waitUntilSubscribeStarts()
        await binding.completeSubscribe(with: .failure(
            RealmServiceError(message: "subscribe failed")
        ))
        do {
            try await subscribe.value
            Issue.record("The controlled subscribe failure must propagate.")
        } catch {
            // Expected.
        }

        bindings.publish(bindingId: binding.id, event: "notes.changed", payload: Data())
        for _ in 0..<5 { await Task.yield() }
        #expect(delivered.isEmpty)
        let suspendCount = await binding.suspendCount
        #expect(suspendCount == 1)
    }
}
