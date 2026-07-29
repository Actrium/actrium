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

    func sendNetworkPathChanged(snapshot: NetworkSnapshot) async {
        networkSnapshots.append(snapshot)
    }

    func sendAppLifecycleChanged(state: AppLifecycleState) async {
        lifecycleStates.append(state)
    }

    func recordedCounts() -> (network: Int, lifecycle: Int) {
        (networkSnapshots.count, lifecycleStates.count)
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

    func sendNetworkPathChanged(snapshot: NetworkSnapshot) async {
        networkSnapshotCount += 1
    }

    func sendAppLifecycleChanged(state: AppLifecycleState) async {
        if state == .background {
            await backgroundStarted.release()
            await releaseBackground.wait()
        }
        states.append(state)
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

@Test func deliverySessionArcTeardownClosesCallbacksRetainedByOldTasks() async {
    let sender = RecordingMobileEventSender()
    let gate = MobileEventDeliveryGate(sender: sender)
    var session: MobileEventDeliverySession? = MobileEventDeliverySession(gate: gate)

    #expect(session != nil)
    session = nil
    await gate.sendAppLifecycleChanged(state: .background)

    let counts = await sender.recordedCounts()
    #expect(counts.lifecycle == 0)
}
