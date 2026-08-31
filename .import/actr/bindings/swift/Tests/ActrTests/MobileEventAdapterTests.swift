@testable import Actr
import ActrBindings
import Foundation
import Network
import Testing

private func transport(
    wifi: Bool = false,
    cellular: Bool = false,
    ethernet: Bool = false,
    vpn: Bool = false,
    other: Bool = false
) -> NetworkTransportFlags {
    NetworkTransportFlags(
        wifi: wifi,
        cellular: cellular,
        ethernet: ethernet,
        vpn: vpn,
        other: other
    )
}

private actor RecordingMobileEventSender: MobileEventSending {
    private(set) var networkSnapshots: [NetworkSnapshot] = []
    private(set) var lifecycleStates: [AppLifecycleState] = []

    func sendNetworkPathChanged(snapshot: NetworkSnapshot) async throws -> MobileEventSendOutcome {
        networkSnapshots.append(snapshot)
        return .completed
    }

    func sendAppLifecycleChanged(state: AppLifecycleState) async throws -> MobileEventSendOutcome {
        lifecycleStates.append(state)
        return .completed
    }

    func recordedCounts() -> (network: Int, lifecycle: Int) {
        (networkSnapshots.count, lifecycleStates.count)
    }
}

private final class RecordingMobileLifecycleObserver: MobileLifecycleObserver, @unchecked Sendable {
    private let lock = NSLock()
    private var recordedEvents: [MobileLifecycleEvent] = []

    var events: [MobileLifecycleEvent] {
        lock.lock()
        defer { lock.unlock() }
        return recordedEvents
    }

    func onMobileLifecycleEvent(_ event: MobileLifecycleEvent) {
        lock.lock()
        recordedEvents.append(event)
        lock.unlock()
    }
}

/// Throws from every send so the gate-level failure fan-out is exercised.
private struct FailingMobileEventSender: MobileEventSending {
    struct NetworkDeliveryError: Error, CustomStringConvertible {
        var description: String { "network error 17" }
    }

    struct LifecycleDeliveryError: Error, CustomStringConvertible {
        var description: String { "lifecycle error 23" }
    }

    func sendNetworkPathChanged(snapshot: NetworkSnapshot) async throws -> MobileEventSendOutcome {
        throw NetworkDeliveryError()
    }

    func sendAppLifecycleChanged(state: AppLifecycleState) async throws -> MobileEventSendOutcome {
        throw LifecycleDeliveryError()
    }
}

/// Reports native processing failures without throwing, matching
/// `NetworkEventResult(success: false, ...)` from the binding.
private struct RejectedMobileEventSender: MobileEventSending {
    func sendNetworkPathChanged(snapshot: NetworkSnapshot) async throws -> MobileEventSendOutcome {
        MobileEventSendOutcome(
            result: NetworkEventResult(
                event: .networkPathChanged(snapshot: snapshot),
                success: false,
                error: "network processing rejected 31",
                durationMs: 3
            )
        )
    }

    func sendAppLifecycleChanged(state: AppLifecycleState) async throws -> MobileEventSendOutcome {
        MobileEventSendOutcome(
            result: NetworkEventResult(
                event: .appLifecycleChanged(state: state),
                success: false,
                error: "lifecycle processing rejected 47",
                durationMs: 5
            )
        )
    }
}

private final class SequenceClock: @unchecked Sendable {
    private let lock = NSLock()
    private var values: [UInt64]

    init(_ values: [UInt64]) {
        self.values = values
    }

    func read() -> UInt64 {
        lock.lock()
        defer { lock.unlock() }
        return values.removeFirst()
    }

    func remaining() -> Int {
        lock.lock()
        defer { lock.unlock() }
        return values.count
    }
}

private actor AsyncLatch {
    private var open = false
    private var waiters: [CheckedContinuation<Void, Never>] = []

    func wait() async {
        guard !open else { return }
        await withCheckedContinuation { continuation in
            waiters.append(continuation)
        }
    }

    func release() {
        guard !open else { return }
        open = true
        let pending = waiters
        waiters.removeAll()
        pending.forEach { $0.resume() }
    }
}

private actor SlowLifecycleSender: MobileEventSending {
    private let backgroundStarted: AsyncLatch
    private let releaseBackground: AsyncLatch
    private var states: [AppLifecycleState] = []
    private var networkSnapshotCount = 0

    init(backgroundStarted: AsyncLatch, releaseBackground: AsyncLatch) {
        self.backgroundStarted = backgroundStarted
        self.releaseBackground = releaseBackground
    }

    func sendNetworkPathChanged(snapshot: NetworkSnapshot) async throws -> MobileEventSendOutcome {
        networkSnapshotCount += 1
        return .completed
    }

    func sendAppLifecycleChanged(state: AppLifecycleState) async throws -> MobileEventSendOutcome {
        if state == .background {
            await backgroundStarted.release()
            await releaseBackground.wait()
        }
        states.append(state)
        return .completed
    }

    func recordedStates() -> [AppLifecycleState] {
        states
    }

    func recordedNetworkSnapshotCount() -> Int {
        networkSnapshotCount
    }
}

@Test func networkPathReducerSuppressesInitialAndDuplicateObservations() {
    var reducer = NetworkPathEventReducer()
    let wifi = NetworkPathObservation(
        status: .satisfied,
        transport: transport(wifi: true),
        isExpensive: false,
        isConstrained: false
    )

    let initial = reducer.reduce(wifi)
    #expect(initial.isInitial)
    #expect(!initial.shouldNotify)
    #expect(initial.snapshot.sequence == 1)
    #expect(initial.snapshot.availability == .available)

    let duplicate = reducer.reduce(wifi)
    #expect(!duplicate.isInitial)
    #expect(!duplicate.shouldNotify)
    #expect(duplicate.snapshot.sequence == 2)

    let forced = reducer.reduce(wifi, forceNotify: true)
    #expect(forced.shouldNotify)
    #expect(forced.snapshot.sequence == 3)
}

@Test func networkPathReducerEmitsForcedInitialObservation() {
    var reducer = NetworkPathEventReducer()
    let offline = NetworkPathObservation(
        status: .unsatisfied,
        transport: transport(),
        isExpensive: false,
        isConstrained: false
    )

    let forcedInitial = reducer.reduce(offline, forceNotify: true)
    #expect(forcedInitial.isInitial)
    #expect(forcedInitial.shouldNotify)
    #expect(forcedInitial.snapshot.sequence == 1)
    #expect(forcedInitial.snapshot.availability == .unavailable)

    let callbackDuplicate = reducer.reduce(offline)
    #expect(!callbackDuplicate.shouldNotify)
}

@Test func networkPathReducerEmitsEveryMaterialSnapshotField() {
    var reducer = NetworkPathEventReducer()
    _ = reducer.reduce(
        NetworkPathObservation(
            status: .satisfied,
            transport: transport(wifi: true),
            isExpensive: false,
            isConstrained: false
        )
    )

    let observations = [
        NetworkPathObservation(
            status: .satisfied,
            transport: transport(cellular: true),
            isExpensive: true,
            isConstrained: false
        ),
        NetworkPathObservation(
            status: .satisfied,
            transport: transport(ethernet: true),
            isExpensive: false,
            isConstrained: false
        ),
        NetworkPathObservation(
            status: .satisfied,
            transport: transport(vpn: true, other: true),
            isExpensive: false,
            isConstrained: true
        ),
        NetworkPathObservation(
            status: .requiresConnection,
            transport: transport(),
            isExpensive: false,
            isConstrained: false
        ),
        NetworkPathObservation(
            status: .unsatisfied,
            transport: transport(),
            isExpensive: false,
            isConstrained: false
        ),
    ]

    let reductions = observations.map { reducer.reduce($0) }
    #expect(reductions.allSatisfy { $0.shouldNotify })
    #expect(reductions.map(\.snapshot.sequence) == [2, 3, 4, 5, 6])
    #expect(reductions[0].snapshot.transport.cellular)
    #expect(reductions[0].snapshot.isExpensive)
    #expect(reductions[1].snapshot.transport.ethernet)
    #expect(reductions[2].snapshot.transport.vpn)
    #expect(reductions[2].snapshot.transport.other)
    #expect(reductions[2].snapshot.isConstrained)
    #expect(reductions[3].snapshot.availability == .unknown)
    #expect(reductions[4].snapshot.availability == .unavailable)
}

@Test func lifecycleReducerPreservesFirstBackgroundTimestampAndClampsClockRollback() {
    var reducer = AppLifecycleEventReducer()
    let epochMs: UInt64 = 1_000_000

    #expect(reducer.willEnterForeground(atMs: epochMs) == .foreground(backgroundDurationMs: 0))
    #expect(reducer.didEnterBackground(atMs: epochMs) == .background)
    #expect(reducer.didEnterBackground(atMs: epochMs + 10_000) == nil)
    #expect(
        reducer.willEnterForeground(atMs: epochMs + 60_000)
            == .foreground(backgroundDurationMs: 60_000)
    )

    #expect(reducer.didEnterBackground(atMs: epochMs + 120_000) == .background)
    #expect(
        reducer.willEnterForeground(atMs: epochMs + 119_000)
            == .foreground(backgroundDurationMs: 0)
    )
}

@Test func lifecycleReducerInitializesOneAuthoritativePhaseWithoutOverwritingEvents() {
    let epochMs: UInt64 = 1_000_000
    var foreground = AppLifecycleEventReducer()
    #expect(
        foreground.initializePhase(isBackground: false, atMs: epochMs)
            == .foreground(backgroundDurationMs: 0)
    )
    #expect(foreground.initializePhase(isBackground: true, atMs: epochMs) == nil)

    var background = AppLifecycleEventReducer()
    #expect(background.initializePhase(isBackground: true, atMs: epochMs) == .background)
    #expect(
        background.willEnterForeground(atMs: epochMs + 5_000)
            == .foreground(backgroundDurationMs: 5_000)
    )

    var observerWonRace = AppLifecycleEventReducer()
    #expect(observerWonRace.didEnterBackground(atMs: epochMs) == .background)
    #expect(observerWonRace.initializePhase(isBackground: false, atMs: epochMs) == nil)
}

@Test func lifecycleEventSourceReadsInjectedSuspendAwareClockAtEachCallSite() {
    let sequence = SequenceClock([10_000, 70_000])
    let source = AppLifecycleEventSource(
        clock: SuspendAwareClock(read: sequence.read)
    )

    #expect(source.didEnterBackground() == .background)
    #expect(
        source.willEnterForeground()
            == .foreground(backgroundDurationMs: 60_000)
    )
    #expect(sequence.remaining() == 0)
}

@Test func serializedLifecycleDeliveryPreventsForegroundOvertakingSlowBackground() async {
    let backgroundStarted = AsyncLatch()
    let releaseBackground = AsyncLatch()
    let sender = SlowLifecycleSender(
        backgroundStarted: backgroundStarted,
        releaseBackground: releaseBackground
    )
    let delivery = SerializedLifecycleDelivery(
        delivery: MobileEventDeliveryGate(sender: sender)
    )

    delivery.enqueue(.background)
    delivery.enqueue(.foreground(backgroundDurationMs: 42))
    await backgroundStarted.wait()
    #expect(await sender.recordedStates().isEmpty)

    await releaseBackground.release()
    await delivery.waitForIdle()
    #expect(
        await sender.recordedStates()
            == [.background, .foreground(backgroundDurationMs: 42)]
    )
}

@Test func explicitNetworkSnapshotWaitsForForegroundDelivery() async {
    let backgroundStarted = AsyncLatch()
    let releaseBackground = AsyncLatch()
    let sender = SlowLifecycleSender(
        backgroundStarted: backgroundStarted,
        releaseBackground: releaseBackground
    )
    let gate = MobileEventDeliveryGate(sender: sender)
    let lifecycleDelivery = SerializedLifecycleDelivery(delivery: gate)
    let snapshot = NetworkSnapshot(
        sequence: 1,
        availability: .available,
        transport: transport(wifi: true),
        isExpensive: false,
        isConstrained: false
    )

    lifecycleDelivery.enqueue(.background)
    let foreground = lifecycleDelivery.enqueue(
        .foreground(backgroundDurationMs: 42)
    )
    let networkDelivery = Task {
        await gate.sendNetworkPathChanged(
            snapshot: snapshot,
            after: foreground
        )
    }

    await backgroundStarted.wait()
    #expect(await sender.recordedNetworkSnapshotCount() == 0)

    await releaseBackground.release()
    await networkDelivery.value
    #expect(
        await sender.recordedStates()
            == [.background, .foreground(backgroundDurationMs: 42)]
    )
    #expect(await sender.recordedNetworkSnapshotCount() == 1)
}

@Test func mobileEventDeliveryGateIgnoresCallbacksAfterClose() async {
    let sender = RecordingMobileEventSender()
    let gate = MobileEventDeliveryGate(sender: sender)
    let snapshot = NetworkSnapshot(
        sequence: 1,
        availability: .available,
        transport: transport(wifi: true),
        isExpensive: false,
        isConstrained: false
    )

    gate.close()
    await gate.sendAppLifecycleChanged(state: .background)
    await gate.sendNetworkPathChanged(snapshot: snapshot)

    let counts = await sender.recordedCounts()
    #expect(counts.lifecycle == 0)
    #expect(counts.network == 0)
}

@Test func deliveryFailureIsObservableThroughMobileLifecycleObserver() async {
    let observer = RecordingMobileLifecycleObserver()
    let emitter = MobileLifecycleEventEmitter(observers: [observer])
    let gate = MobileEventDeliveryGate(
        sender: FailingMobileEventSender(),
        emitter: emitter
    )
    let snapshot = NetworkSnapshot(
        sequence: 3,
        availability: .available,
        transport: transport(cellular: true),
        isExpensive: true,
        isConstrained: false
    )

    await gate.sendNetworkPathChanged(snapshot: snapshot)
    await gate.sendAppLifecycleChanged(state: .background)

    guard observer.events.count == 2 else {
        Issue.record("Expected exactly two delivery events, got: \(observer.events)")
        return
    }

    guard case let .networkDeliveryFailed(receivedSnapshot, networkMessage) = observer.events[0] else {
        Issue.record("Expected network delivery failure event")
        return
    }
    #expect(receivedSnapshot == snapshot)
    #expect(networkMessage.contains("17"))

    guard case let .lifecycleDeliveryFailed(receivedState, lifecycleMessage) = observer.events[1] else {
        Issue.record("Expected lifecycle delivery failure event")
        return
    }
    #expect(receivedState == .background)
    #expect(lifecycleMessage.contains("23"))
}

@Test func reportedNativeFailureIsObservableThroughMobileLifecycleObserver() async {
    let observer = RecordingMobileLifecycleObserver()
    let emitter = MobileLifecycleEventEmitter(observers: [observer])
    let gate = MobileEventDeliveryGate(
        sender: RejectedMobileEventSender(),
        emitter: emitter
    )
    let snapshot = NetworkSnapshot(
        sequence: 5,
        availability: .available,
        transport: transport(wifi: true),
        isExpensive: false,
        isConstrained: false
    )

    await gate.sendNetworkPathChanged(snapshot: snapshot)
    await gate.sendAppLifecycleChanged(state: .background)

    guard observer.events.count == 2 else {
        Issue.record("Expected exactly two delivery events, got: \(observer.events)")
        return
    }

    guard case let .networkDeliveryFailed(receivedSnapshot, networkMessage) = observer.events[0] else {
        Issue.record("Expected reported network delivery failure event")
        return
    }
    #expect(receivedSnapshot == snapshot)
    #expect(networkMessage.contains("31"))

    guard case let .lifecycleDeliveryFailed(receivedState, lifecycleMessage) = observer.events[1] else {
        Issue.record("Expected reported lifecycle delivery failure event")
        return
    }
    #expect(receivedState == .background)
    #expect(lifecycleMessage.contains("47"))
}

@Test func deliveryDiscardAfterCloseCarriesDiscardedPayload() async {
    let observer = RecordingMobileLifecycleObserver()
    let emitter = MobileLifecycleEventEmitter(observers: [observer])
    let sender = RecordingMobileEventSender()
    let gate = MobileEventDeliveryGate(sender: sender, emitter: emitter)
    let snapshot = NetworkSnapshot(
        sequence: 7,
        availability: .unavailable,
        transport: transport(),
        isExpensive: false,
        isConstrained: false
    )

    gate.close()
    await gate.sendAppLifecycleChanged(state: .background)
    await gate.sendNetworkPathChanged(snapshot: snapshot)

    #expect(observer.events == [
        .lifecycleDeliveryDiscardedAfterClose(.background),
        .networkDeliveryDiscardedAfterClose(snapshot)
    ])
    #expect(await sender.recordedCounts().network == 0)
}

@Test func deliverySessionArcTeardownClosesCallbacksRetainedByOldTasks() async {
    let observer = RecordingMobileLifecycleObserver()
    var emitter: MobileLifecycleEventEmitter? = MobileLifecycleEventEmitter(observers: [observer])
    let sender = RecordingMobileEventSender()
    let gate = MobileEventDeliveryGate(sender: sender, emitter: emitter)
    var session: MobileEventDeliverySession? = MobileEventDeliverySession(gate: gate)

    #expect(session != nil)
    emitter = nil
    session = nil
    await gate.sendAppLifecycleChanged(state: .background)

    let counts = await sender.recordedCounts()
    #expect(counts.lifecycle == 0)
    #expect(observer.events == [.lifecycleDeliveryDiscardedAfterClose(.background)])
}
