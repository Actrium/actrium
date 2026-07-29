import ActrBindings
import Foundation
import Network

protocol MobileEventSending: Sendable {
    func sendNetworkPathChanged(snapshot: NetworkSnapshot) async
    func sendAppLifecycleChanged(state: AppLifecycleState) async
}

private final class BindingMobileEventSender: MobileEventSending, @unchecked Sendable {
    private let handle: NetworkEventHandleWrapper

    init(handle: NetworkEventHandleWrapper) {
        self.handle = handle
    }

    func sendNetworkPathChanged(snapshot: NetworkSnapshot) async {
        _ = try? await handle.handleNetworkPathChanged(snapshot: snapshot)
    }

    func sendAppLifecycleChanged(state: AppLifecycleState) async {
        _ = try? await handle.handleAppLifecycleChanged(state: state)
    }
}

final class MobileEventDeliveryGate: @unchecked Sendable {
    private let sender: any MobileEventSending
    private let stateLock = NSLock()
    private var closed = false

    convenience init(handle: NetworkEventHandleWrapper) {
        self.init(sender: BindingMobileEventSender(handle: handle))
    }

    init(sender: any MobileEventSending) {
        self.sender = sender
    }

    func close() {
        stateLock.lock()
        closed = true
        stateLock.unlock()
    }

    func sendNetworkPathChanged(
        snapshot: NetworkSnapshot,
        after predecessor: Task<Void, Never>? = nil
    ) async {
        await predecessor?.value
        guard isOpen() else { return }
        await sender.sendNetworkPathChanged(snapshot: snapshot)
    }

    func sendAppLifecycleChanged(state: AppLifecycleState) async {
        guard isOpen() else { return }
        await sender.sendAppLifecycleChanged(state: state)
    }

    private func isOpen() -> Bool {
        stateLock.lock()
        defer { stateLock.unlock() }
        return !closed
    }
}

/// Owns one platform-monitor delivery incarnation. Tasks may retain `gate`
/// after the monitor is released; ARC teardown closes it before those stale
/// callbacks can enter the binding.
final class MobileEventDeliverySession {
    let gate: MobileEventDeliveryGate

    convenience init(handle: NetworkEventHandleWrapper) {
        self.init(gate: MobileEventDeliveryGate(handle: handle))
    }

    init(gate: MobileEventDeliveryGate) {
        self.gate = gate
    }

    deinit {
        gate.close()
    }
}

/// Serializes lifecycle sends without blocking the platform callback queue.
/// A later foreground event cannot overtake a slow background delivery.
final class SerializedLifecycleDelivery: @unchecked Sendable {
    private let delivery: MobileEventDeliveryGate
    private let stateLock = NSLock()
    private var tail: Task<Void, Never>?

    init(delivery: MobileEventDeliveryGate) {
        self.delivery = delivery
    }

    @discardableResult
    func enqueue(_ state: AppLifecycleState) -> Task<Void, Never> {
        stateLock.lock()
        let previous = tail
        let task = Task { [delivery] in
            await previous?.value
            await delivery.sendAppLifecycleChanged(state: state)
        }
        tail = task
        stateLock.unlock()
        return task
    }

    func waitForIdle() async {
        await currentTail()?.value
    }

    private func currentTail() -> Task<Void, Never>? {
        stateLock.lock()
        defer { stateLock.unlock() }
        return tail
    }
}

struct NetworkPathObservation: Equatable {
    let status: NWPath.Status
    let transport: NetworkTransportFlags
    let isExpensive: Bool
    let isConstrained: Bool

    var availability: NetworkAvailability {
        switch status {
        case .satisfied:
            return .available
        case .unsatisfied:
            return .unavailable
        case .requiresConnection:
            return .unknown
        @unknown default:
            return .unknown
        }
    }
}

struct NetworkPathReduction {
    let snapshot: NetworkSnapshot
    let isInitial: Bool
    let shouldNotify: Bool
}

struct NetworkPathEventReducer {
    private var lastObservation: NetworkPathObservation?
    private var nextSequence: UInt64 = 1

    mutating func reduce(
        _ observation: NetworkPathObservation,
        forceNotify: Bool = false
    ) -> NetworkPathReduction {
        let snapshot = NetworkSnapshot(
            sequence: nextSequence,
            availability: observation.availability,
            transport: observation.transport,
            isExpensive: observation.isExpensive,
            isConstrained: observation.isConstrained
        )
        nextSequence += 1

        let isInitial = lastObservation == nil
        let shouldNotify = forceNotify || (!isInitial && lastObservation != observation)
        lastObservation = observation

        return NetworkPathReduction(
            snapshot: snapshot,
            isInitial: isInitial,
            shouldNotify: shouldNotify
        )
    }
}

/// Milliseconds on a suspend-aware monotonic timeline.
///
/// The background duration feeds the runtime's short/long-background decision,
/// so it must keep counting while the device sleeps and must ignore wall-clock
/// adjustments (NTP, manual changes). On Darwin, `CLOCK_MONOTONIC` advances
/// across sleep; on Linux, `CLOCK_BOOTTIME` is the equivalent.
struct SuspendAwareClock: Sendable {
    private let read: @Sendable () -> UInt64

    init(read: @escaping @Sendable () -> UInt64) {
        self.read = read
    }

    func nowMs() -> UInt64 {
        read()
    }

    static let system = SuspendAwareClock {
        #if canImport(Darwin)
        return clock_gettime_nsec_np(CLOCK_MONOTONIC) / 1_000_000
        #else
        var ts = timespec()
        clock_gettime(CLOCK_BOOTTIME, &ts)
        return UInt64(ts.tv_sec) * 1_000 + UInt64(ts.tv_nsec) / 1_000_000
        #endif
    }
}

/// Owns the lifecycle reducer and reads every callback timestamp through one
/// injected suspend-aware clock. Tests can therefore catch a call-site revert
/// to wall time, not merely prove the reducer's arithmetic.
final class AppLifecycleEventSource {
    private let clock: SuspendAwareClock
    private var reducer = AppLifecycleEventReducer()

    init(clock: SuspendAwareClock = .system) {
        self.clock = clock
    }

    func initializePhase(isBackground: Bool) -> AppLifecycleState? {
        reducer.initializePhase(isBackground: isBackground, atMs: clock.nowMs())
    }

    func didEnterBackground() -> AppLifecycleState? {
        reducer.didEnterBackground(atMs: clock.nowMs())
    }

    func willEnterForeground() -> AppLifecycleState {
        reducer.willEnterForeground(atMs: clock.nowMs())
    }
}

/// Reduces app lifecycle callbacks into `AppLifecycleState` values.
///
/// Timestamps are suspend-aware monotonic milliseconds (`SuspendAwareClock`);
/// entering background and returning to foreground must use the same clock
/// source or the reported duration is meaningless.
struct AppLifecycleEventReducer {
    private var backgroundedAtMs: UInt64?
    private var hasLifecycleObservation = false

    mutating func initializePhase(isBackground: Bool, atMs now: UInt64) -> AppLifecycleState? {
        guard !hasLifecycleObservation else {
            return nil
        }
        hasLifecycleObservation = true
        if isBackground {
            backgroundedAtMs = now
            return .background
        }
        backgroundedAtMs = nil
        return .foreground(backgroundDurationMs: 0)
    }

    mutating func didEnterBackground(atMs now: UInt64) -> AppLifecycleState? {
        hasLifecycleObservation = true
        guard backgroundedAtMs == nil else {
            return nil
        }
        backgroundedAtMs = now
        return .background
    }

    mutating func willEnterForeground(atMs now: UInt64) -> AppLifecycleState {
        hasLifecycleObservation = true
        guard let backgroundedAtMs else {
            return .foreground(backgroundDurationMs: 0)
        }

        self.backgroundedAtMs = nil
        // A monotonic clock never runs backwards; the clamp only keeps an
        // unexpected reversal from wrapping the unsigned subtraction.
        let elapsedMs = now >= backgroundedAtMs ? now - backgroundedAtMs : 0
        return .foreground(backgroundDurationMs: elapsedMs)
    }
}
