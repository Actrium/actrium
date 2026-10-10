@testable import Actr
import ActrBindings
import Foundation
import Testing

/// Scripted in-memory bridge for driving `DataChunkSequence` without a live
/// runtime. `hang` steps park `next()` until `close()` releases them with
/// `nil`, mirroring the real bridge's wake-on-close contract.
private final class MockStreamBridge: DataChunkStreamBridging, @unchecked Sendable {
    enum Step {
        /// One delivered chunk (sequence carried for assertion).
        case event(UInt64)
        /// Stream end: `next()` returns nil.
        case end
        /// Park the reader; released (with nil) by `close()`.
        case hang
    }

    private let lock = NSLock()
    private var steps: [Step]
    private var pending: CheckedContinuation<ActrBindings.DataChunkEventBridge?, Never>?
    private var closeWaiters: [CheckedContinuation<Void, Never>] = []
    private(set) var closeCalls = 0

    init(steps: [Step]) {
        self.steps = steps
    }

    /// Resolves once `close()` has been called at least once.
    func waitForClose() async {
        await withCheckedContinuation { continuation in
            lock.withLock {
                if closeCalls > 0 {
                    continuation.resume()
                } else {
                    closeWaiters.append(continuation)
                }
            }
        }
    }

    func next() async throws -> ActrBindings.DataChunkEventBridge? {
        let step: Step? = lock.withLock {
            steps.isEmpty ? nil : steps.removeFirst()
        }
        guard let step else {
            // Nothing scripted: park like a quiet stream.
            return await withCheckedContinuation { continuation in
                lock.withLock { pending = continuation }
            }
        }
        switch step {
        case .event(let sequence):
            return ActrBindings.DataChunkEventBridge(
                chunk: ActrBindings.DataChunk(
                    streamId: "s",
                    sequence: sequence,
                    payload: Data(),
                    metadata: [],
                    timestampMs: nil,
                    endOfStream: nil
                ),
                sender: ActrBindings.ActrId(
                    realm: ActrBindings.Realm(realmId: 1),
                    serialNumber: 1,
                    type: ActrBindings.ActrType(
                        manufacturer: "test", name: "fixture", version: "0.1.0"
                    )
                )
            )
        case .end:
            return nil
        case .hang:
            return await withCheckedContinuation { continuation in
                lock.withLock { pending = continuation }
            }
        }
    }

    func close() async {
        lock.withLock {
            closeCalls += 1
            if let pending {
                self.pending = nil
                pending.resume(returning: nil)
            }
            for waiter in closeWaiters {
                waiter.resume()
            }
            closeWaiters.removeAll()
        }
    }
}

@Suite(.serialized)
struct DataChunkSequenceTests {
    @Test
    func yieldsScriptedEventsThenEnds() async throws {
        let bridge = MockStreamBridge(steps: [.event(1), .event(2), .end])
        let sequence = DataChunkSequence(bridge: bridge)
        var iterator = sequence.makeAsyncIterator()
        #expect(try await iterator.next()?.chunk.sequence == 1)
        #expect(try await iterator.next()?.chunk.sequence == 2)
        // Stream end becomes nil, and the iterator stays finished.
        #expect(try await iterator.next() == nil)
        #expect(try await iterator.next() == nil)
        #expect(bridge.closeCalls == 0)
    }

    @Test
    func secondIteratorIsRejected() async throws {
        let bridge = MockStreamBridge(steps: [.event(1)])
        let sequence = DataChunkSequence(bridge: bridge)
        var first = sequence.makeAsyncIterator()
        var second = sequence.makeAsyncIterator()
        do {
            _ = try await second.next()
            Issue.record("second iterator must be rejected")
        } catch let error as Actr.ActrError {
            guard case .InvalidArgument = error else {
                Issue.record("expected InvalidArgument, got \(error)")
                return
            }
        }
        // The rejected second iterator did not kill the first one.
        #expect(try await first.next()?.chunk.sequence == 1)
    }

    @Test
    func closeWakesPendingReadWithNil() async throws {
        let bridge = MockStreamBridge(steps: [.hang])
        let sequence = DataChunkSequence(bridge: bridge)
        var iterator = sequence.makeAsyncIterator()

        let read = Task { try await iterator.next() }
        // Let the read park on the bridge.
        try await Task.sleep(for: .milliseconds(50))
        await sequence.close()

        #expect(try await read.value == nil)
        #expect(bridge.closeCalls == 1)
    }

    @Test
    func cancellingTheReadClosesTheBridge() async throws {
        let bridge = MockStreamBridge(steps: [.hang])
        let sequence = DataChunkSequence(bridge: bridge)
        var iterator = sequence.makeAsyncIterator()

        let read = Task { try await iterator.next() }
        try await Task.sleep(for: .milliseconds(50))
        read.cancel()

        _ = try? await read.value
        // The cancellation handler closes the bridge from an independent
        // task; await the signal with a deadline instead of sleeping blind.
        try await withThrowingTaskGroup(of: Bool.self) { group in
            group.addTask {
                await bridge.waitForClose()
                return true
            }
            group.addTask {
                try await Task.sleep(for: .seconds(5))
                return false
            }
            let observed = try await group.next() ?? false
            group.cancelAll()
            #expect(observed, "bridge.close() must run after the read is cancelled")
        }
    }
}
