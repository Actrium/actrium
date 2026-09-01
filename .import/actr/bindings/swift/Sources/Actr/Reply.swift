import Foundation

/// Movable response handle for generated EXPLICIT RPC handlers.
///
/// The handle may outlive its handler. If it is never sent, the host expires
/// the pending reply at the RFC-0323 in-flight TTL.
public final class Reply<Response>: @unchecked Sendable {
    private let context: any ActrContext
    private let encode: (Response) throws(ActrError) -> Data

    public init(
        ctx: any ActrContext,
        encode: @escaping (Response) throws(ActrError) -> Data
    ) {
        self.context = ctx
        self.encode = encode
    }

    /// Deliver one response through the current invocation context.
    public func send(_ response: Response) async throws(ActrError) {
        try await context.replyRaw(payload: encode(response))
    }

    /// Deliver one ActrError through the current invocation context.
    public func sendError(_ error: ActrError) async throws(ActrError) {
        try await context.replyError(error)
    }
}
