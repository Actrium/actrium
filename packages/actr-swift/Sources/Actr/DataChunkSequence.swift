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

/// Seam over the UniFFI stream bridge so the sequence lifecycle is unit-testable
/// without a live runtime.
protocol DataChunkStreamBridging: Sendable {
    func next() async throws -> ActrBindings.DataChunkEventBridge?
    func close() async throws
    func endedByMarker() -> Bool
    func endMarker() -> ActrBindings.DataChunkEventBridge?
}

extension ActrBindings.DataChunkStreamBridge: DataChunkStreamBridging {}

/// Pull-based `AsyncSequence` over one DataChunk subscription.
///
/// The iterator calls the opaque bridge's `next()` directly — there is no
/// continuation-driven second buffer, so queue capacity stays the single
/// bound configured at subscription time.
///
/// Graceful end: when the peer's sink closes it sends an in-band
/// end-of-stream marker; the sequence yields every chunk queued before it,
/// then ends (`nil`). On latency-first lanes the marker can be dropped like
/// any other chunk, so back completion with an application-level timeout
/// there.
///
/// The sequence is single-consumer: iterating it twice throws
/// `ActrError.InvalidArgument` on the second iterator's first `next()`.
/// `close()` (called on cancellation or explicit termination) closes the
/// bridge, unregisters the subscription, and wakes a pending `next()` with
/// `nil`.
public struct DataChunkSequence: AsyncSequence, Sendable {
    public typealias Element = DataChunkEvent

    private let bridge: any DataChunkStreamBridging
    private let state: State

    init(bridge: any DataChunkStreamBridging) {
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

    /// Whether the stream ended on the peer's in-band end-of-stream marker
    /// (as opposed to a local `close()` or runtime teardown). Meaningful
    /// after iteration returned `nil`; survives `close()`.
    public var endedByMarker: Bool {
        bridge.endedByMarker()
    }

    /// The end-of-stream marker event that terminated the stream, if any.
    /// The sender is the transport-authenticated peer that sent it, so a
    /// consumer that pins a stream to one expected producer can detect a
    /// foreign terminator here.
    public var endMarker: DataChunkEvent? {
        bridge.endMarker().map {
            DataChunkEvent(chunk: DataChunk(bridge: $0.chunk), sender: ActrId(bridge: $0.sender))
        }
    }

    public struct Iterator: AsyncIteratorProtocol {
        private let bridge: any DataChunkStreamBridging
        private let usable: Bool
        private var finished = false

        init(bridge: any DataChunkStreamBridging, usable: Bool) {
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
