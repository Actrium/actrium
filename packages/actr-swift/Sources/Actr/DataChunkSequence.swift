import ActrBindings

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
        var iteratorVended = false
    }

    public func makeAsyncIterator() -> Iterator {
        let firstConsumer = !state.iteratorVended
        state.iteratorVended = true
        return Iterator(bridge: bridge, usable: firstConsumer)
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
                guard let event = try await bridge.next() else {
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
