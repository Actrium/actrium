import ActrBindings
import Foundation

/// One inbound DataChunk together with its sender.
public struct DataChunkEvent: Sendable {
    public let chunk: DataChunk
    public let sender: ActrId

    public init(chunk: DataChunk, sender: ActrId) {
        self.chunk = chunk
        self.sender = sender
    }
}

/// Pull-based `AsyncSequence` over one DataChunk subscription.
///
/// The iterator calls the opaque bridge's `next()` directly — there is no
/// continuation-driven second buffer, so queue capacity stays the single
/// bound configured at subscription time.
///
/// The sequence is single-consumer: iterating it twice throws
/// `ActrError.InvalidArgument` on the second iterator's first `next()`.
/// `close()` (called on cancellation or explicit termination) closes the
/// bridge, unregisters the subscription, and wakes a pending `next()` with
/// `nil`.
public struct DataChunkSequence: AsyncSequence, Sendable {
    public typealias Element = DataChunkEvent

    private let bridge: ActrBindings.DataChunkStreamBridge
    private let state: State

    init(bridge: ActrBindings.DataChunkStreamBridge) {
        self.bridge = bridge
        self.state = State()
    }

    final class State: @unchecked Sendable {
        private let lock = NSLock()
        private var vended = false

        /// True for the first caller only; every later caller gets `false`.
        func claimIterator() -> Bool {
            lock.lock()
            defer { lock.unlock() }
            if vended { return false }
            vended = true
            return true
        }
    }

    public func makeAsyncIterator() -> Iterator {
        Iterator(bridge: bridge, usable: state.claimIterator())
    }

    /// Close the subscription. Idempotent; wakes a pending `next()` with `nil`.
    public func close() async {
        try? await bridge.close()
    }

    public struct Iterator: AsyncIteratorProtocol {
        private let bridge: ActrBindings.DataChunkStreamBridge
        private let usable: Bool
        private var finished = false

        init(bridge: ActrBindings.DataChunkStreamBridge, usable: Bool) {
            self.bridge = bridge
            self.usable = usable
        }

        public mutating func next() async throws(ActrError) -> DataChunkEvent? {
            if finished {
                return nil
            }
            guard usable else {
                finished = true
                throw ActrError.InvalidArgument(
                    msg: "DataChunkSequence is single-consumer"
                )
            }
            do {
                let bridge = self.bridge
                let event = try await withTaskCancellationHandler {
                    try await bridge.next()
                } onCancel: {
                    // Use an independent task: cleanup must still run after the
                    // consuming task's pending UniFFI future is cancelled.
                    Task { try? await bridge.close() }
                }
                guard let event else {
                    finished = true
                    return nil
                }
                return DataChunkEvent(
                    chunk: DataChunk(bridge: event.chunk),
                    sender: ActrId(bridge: event.sender)
                )
            } catch {
                finished = true
                throw ActrError(error: error)
            }
        }
    }
}
