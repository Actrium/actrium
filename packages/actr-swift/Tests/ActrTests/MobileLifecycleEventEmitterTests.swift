@testable import Actr
import Foundation
import Testing

private final class RecordingLifecycleObserver: MobileLifecycleObserver, @unchecked Sendable {
    private let lock = NSLock()
    private var events: [MobileLifecycleEvent] = []

    var recordedEvents: [MobileLifecycleEvent] {
        lock.lock()
        defer { lock.unlock() }
        return events
    }

    func onMobileLifecycleEvent(_ event: MobileLifecycleEvent) {
        lock.lock()
        events.append(event)
        lock.unlock()
    }
}

@Test func emitterFansOutEveryEventToEveryObserver() {
    let first = RecordingLifecycleObserver()
    let second = RecordingLifecycleObserver()
    let emitter = MobileLifecycleEventEmitter(observers: [first, second])
    let event = MobileLifecycleEvent.monitorInitialized(.network)

    emitter.emit(event)

    #expect(first.recordedEvents == [event])
    #expect(second.recordedEvents == [event])
}

@Test func emitterDoesNotRetainObservers() {
    weak var weakObserver: RecordingLifecycleObserver?
    var observer: RecordingLifecycleObserver? = RecordingLifecycleObserver()
    weakObserver = observer
    let emitter = MobileLifecycleEventEmitter(
        observers: observer.map { [$0 as any MobileLifecycleObserver] } ?? []
    )

    observer = nil

    #expect(weakObserver == nil)
    emitter.emit(.monitorInitialized(.network))
}
