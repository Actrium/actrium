import ActrBindings

/// Outbound DataChunk sink bound to one destination, stream ID, and lane.
///
/// Sends are serialized by the bridge in acceptance order. `close()` stops
/// accepting new sends, waits for accepted sends to finish, and is
/// idempotent; sending after close throws `ActrError.InvalidArgument`.
/// Every chunk's `streamId` must equal the sink's stream ID.
public final class DataChunkSink: Sendable {
    private let bridge: ActrBindings.DataChunkSinkBridge

    init(bridge: ActrBindings.DataChunkSinkBridge) {
        self.bridge = bridge
    }

    public func send(_ chunk: DataChunk) async throws(ActrError) {
        do {
            try await bridge.send(chunk: chunk.bridge)
        } catch {
            throw ActrError(error: error)
        }
    }

    public func close() async throws(ActrError) {
        do {
            try await bridge.close()
        } catch {
            throw ActrError(error: error)
        }
    }
}
