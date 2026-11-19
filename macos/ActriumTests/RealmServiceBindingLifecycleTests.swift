import ActrBindings
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

    var attachmentUploadAvailable: Bool { false }

    func prepareAttachmentUpload(
        _ request: RealmServiceAttachmentPrepareCall,
        deliver: @escaping @Sendable (RealmServiceAttachmentResultEvent) -> Void
    ) async -> RealmServiceAttachmentPrepareReply {
        RealmServiceAttachmentPrepareReply(
            id: request.id,
            failure: RealmServiceFailure(code: "unavailable", message: "Test binding")
        )
    }

    func sendAttachmentChunk(
        _ request: RealmServiceAttachmentChunkCall
    ) async -> RealmServiceAttachmentChunkReply {
        RealmServiceAttachmentChunkReply(
            id: request.id,
            failure: RealmServiceFailure(code: "unavailable", message: "Test binding")
        )
    }

    func unregisterAttachmentStream(
        _ request: RealmServiceAttachmentUnregisterCall
    ) async -> RealmServiceAttachmentUnregisterReply {
        RealmServiceAttachmentUnregisterReply(
            id: request.id,
            failure: RealmServiceFailure(code: "unavailable", message: "Test binding")
        )
    }

    func cancelAttachmentUploads() async {}

    func stop() {}
}

private final class ControlledEventReference: RealmServiceEventReference, @unchecked Sendable {
    private struct State {
        var target: ActrBindings.ActrId
        var peer: ActrBindings.ActrId?
        var discovery: CheckedContinuation<[ActrBindings.ActrId], Error>?
        var discoveryCount = 0
    }

    private let lock = NSLock()
    private var state: State

    init(target: ActrBindings.ActrId) {
        state = State(target: target)
    }

    var peer: ActrBindings.ActrId? {
        lock.lock()
        defer { lock.unlock() }
        return state.peer
    }

    private var discoveryCount: Int {
        lock.lock()
        defer { lock.unlock() }
        return state.discoveryCount
    }

    func discover(
        targetType: ActrBindings.ActrType,
        count: UInt32
    ) async throws -> [ActrBindings.ActrId] {
        try await withCheckedThrowingContinuation { continuation in
            lock.lock()
            state.discovery = continuation
            state.discoveryCount += 1
            lock.unlock()
        }
    }

    func waitForDiscovery() async {
        while discoveryCount == 0 { await Task.yield() }
    }

    func completeDiscovery() {
        lock.lock()
        let discovery = state.discovery
        let target = state.target
        state.discovery = nil
        lock.unlock()
        discovery?.resume(returning: [target])
    }

    func setServiceEventPeer(peer: ActrBindings.ActrId?) {
        lock.lock()
        state.peer = peer
        lock.unlock()
    }
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

    @Test func rendererReplacementRetiresAnEstablishedSubscription() async throws {
        let bindings = RealmServiceBindings()
        let binding = ControlledRealmServiceBinding()
        let tabId = UUID()
        let oldId = UUID()
        let recoveredId = UUID()
        var delivered: [UUID] = []
        bindings.attach(binding, to: tabId)
        bindings.eventHandler = { _, event in delivered.append(event.subscriptionId) }
        await binding.setCompletesSubscribeImmediately()
        try await bindings.subscribe(
            RealmServiceEventSubscription(id: oldId, events: ["notes.changed"]), tabId: tabId
        )

        // releaseRenderer(for:) and renderer termination both use this hook;
        // crash recovery then reuses the same tab ID with a new subscription.
        bindings.retireSubscriptions(for: tabId)
        bindings.publish(bindingId: binding.id, event: "notes.changed", payload: Data())
        for _ in 0..<5 { await Task.yield() }
        #expect(delivered.isEmpty)

        try await bindings.subscribe(
            RealmServiceEventSubscription(id: recoveredId, events: ["notes.changed"]), tabId: tabId
        )
        bindings.publish(bindingId: binding.id, event: "notes.changed", payload: Data())
        for _ in 0..<20 where delivered.isEmpty { await Task.yield() }
        #expect(delivered == [recoveredId])
    }

    @Test func rendererReplacementRetiresAPendingSubscription() async throws {
        let bindings = RealmServiceBindings()
        let binding = ControlledRealmServiceBinding()
        let tabId = UUID()
        let oldId = UUID()
        let recoveredId = UUID()
        var delivered: [UUID] = []
        bindings.attach(binding, to: tabId)
        bindings.eventHandler = { _, event in delivered.append(event.subscriptionId) }
        let subscribe = Task {
            try await bindings.subscribe(
                RealmServiceEventSubscription(id: oldId, events: ["notes.changed"]), tabId: tabId
            )
        }
        await binding.waitUntilSubscribeStarts()

        bindings.retireSubscriptions(for: tabId)
        await binding.completeSubscribe(with: .success(()))
        do {
            try await subscribe.value
            Issue.record("A retired renderer's pending subscribe must not complete.")
        } catch is CancellationError {
            // Expected.
        }

        await binding.setCompletesSubscribeImmediately()
        try await bindings.subscribe(
            RealmServiceEventSubscription(id: recoveredId, events: ["notes.changed"]), tabId: tabId
        )
        bindings.publish(bindingId: binding.id, event: "notes.changed", payload: Data())
        for _ in 0..<20 where delivered.isEmpty { await Task.yield() }
        #expect(delivered == [recoveredId])
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

    @Test func olderDiscoveryCannotReplaceANewerEventPeer() async throws {
        let controller = RealmServiceEventPeerController()
        let oldTarget = ActrBindings.ActrId(realm: "realm@example.test", serialNumber: 1)
        let newTarget = ActrBindings.ActrId(realm: "realm@example.test", serialNumber: 2)
        let oldReference = ControlledEventReference(target: oldTarget)
        let newReference = ControlledEventReference(target: newTarget)
        let serviceType = ActrBindings.ActrType(publisher: "pub", name: "svc", version: "1.0.0")

        let oldSelection = Task {
            try await controller.select(generation: 1, reference: oldReference, serviceType: serviceType)
        }
        await oldReference.waitForDiscovery()
        let newSelection = Task {
            try await controller.select(generation: 2, reference: newReference, serviceType: serviceType)
        }
        await newReference.waitForDiscovery()

        // Complete the discoveries in reverse order. The late generation 1
        // result must fail instead of replacing generation 2's peer.
        newReference.completeDiscovery()
        try await newSelection.value
        oldReference.completeDiscovery()
        do {
            try await oldSelection.value
            Issue.record("An older discovery must not commit its event peer.")
        } catch is CancellationError {
            // Expected.
        }
        #expect(oldReference.peer == nil)
        #expect(newReference.peer == newTarget)
        let selectedPeer = await controller.peer
        #expect(selectedPeer == newTarget)
    }

    @Test func callRediscoveryRefreshesAnActiveEventPeer() async throws {
        let controller = RealmServiceEventPeerController()
        let originalTarget = ActrBindings.ActrId(realm: "realm@example.test", serialNumber: 1)
        let restartedTarget = ActrBindings.ActrId(realm: "realm@example.test", serialNumber: 2)
        let reference = ControlledEventReference(target: originalTarget)
        let serviceType = ActrBindings.ActrType(publisher: "pub", name: "svc", version: "1.0.0")
        let selection = Task {
            try await controller.select(generation: 1, reference: reference, serviceType: serviceType)
        }
        await reference.waitForDiscovery()
        reference.completeDiscovery()
        try await selection.value
        #expect(reference.peer == originalTarget)

        // This is the call(_:) path after discovery returns a restarted Service
        // with a new ActorId while the Front and its subscription stay alive.
        let activeGeneration = await controller.activeGeneration()
        let generation = try #require(activeGeneration)
        await controller.refresh(
            ifGeneration: generation, reference: reference, target: restartedTarget
        )
        #expect(reference.peer == restartedTarget)
        let selectedPeer = await controller.peer
        #expect(selectedPeer == restartedTarget)

        // A delayed refresh from before the last unsubscribe must not revive it.
        await controller.suspend(ifGeneration: 1)
        await controller.refresh(
            ifGeneration: 1, reference: reference, target: restartedTarget
        )
        #expect(reference.peer == nil)
    }
}
