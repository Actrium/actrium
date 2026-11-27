import Foundation

@MainActor
public final class RealmServiceClient {
    private final class SendState: @unchecked Sendable {
        let deadline: DispatchTime
        private let lock = NSLock()
        private var cancelled = false

        init(deadline: DispatchTime) { self.deadline = deadline }

        func canSend() -> Bool {
            lock.withLock { !cancelled && .now() < deadline }
        }

        func cancel() {
            lock.withLock { cancelled = true }
        }
    }

    private struct Pending {
        let tabId: String
        let state: SendState
        let reply: (RealmServiceReply) -> Void
    }

    private let connection: IPCHelperConnection
    private var pending: [UUID: Pending] = [:]

    public init(connection: IPCHelperConnection) {
        self.connection = connection
    }

    public func send(_ call: RealmServiceCall, tabId: String, reply: @escaping (RealmServiceReply) -> Void) {
        let id = call.id
        guard (1...300_000).contains(call.timeoutMs) else {
            reply(Self.failure(id, code: "invalid_service_call", "Invalid Service call timeout."))
            return
        }
        guard pending.count < 128, pending[id] == nil else {
            reply(Self.failure(id, code: "realm_service_busy", "Too many pending Service calls."))
            return
        }
        let state = SendState(deadline: .now() + .milliseconds(call.timeoutMs))
        pending[id] = Pending(tabId: tabId, state: state, reply: reply)
        DispatchQueue.main.asyncAfter(deadline: state.deadline) { [weak self] in
            self?.complete(Self.failure(id, code: "realm_service_timeout", "The Service call timed out."))
        }
        do {
            try connection.enqueueHostEvent(
                name: RealmServiceCall.requestEvent, tabId: tabId, retainedBytes: Self.retainedBytes(call)
            ) {
                guard state.canSend() else { return nil }
                let encoder = JSONEncoder()
                encoder.outputFormatting = .withoutEscapingSlashes
                return try String(decoding: encoder.encode(call), as: UTF8.self)
            } beginSend: {
                // Check again after encoding so an expired request cannot start writing.
                state.canSend()
            } completion: { [weak self] error in
                guard let error else { return }
                DispatchQueue.main.async { [weak self] in self?.sendFailed(id: id, error: error) }
            }
        } catch {
            sendFailed(id: id, error: error)
        }
    }

    public func receive(json: String, tabId: String) {
        guard let reply = try? JSONDecoder().decode(RealmServiceReply.self, from: Data(json.utf8)),
              pending[reply.id]?.tabId == tabId else { return }
        complete(reply)
    }

    public func close(tabId: String) {
        for (id, request) in pending where request.tabId == tabId {
            complete(Self.failure(id, code: "realm_tab_closed", "The tab was closed."))
        }
    }

    private func sendFailed(id: UUID, error: Error) {
        if case IPCError.sendQueueFull = error {
            complete(Self.failure(id, code: "realm_service_busy", "Too many pending Service calls."))
        } else {
            complete(RealmServiceReply(id: id, failure: RealmServiceFailure(error: error)))
        }
    }

    private func complete(_ reply: RealmServiceReply) {
        guard let request = pending.removeValue(forKey: reply.id) else { return }
        request.state.cancel()
        request.reply(reply)
    }

    private static func retainedBytes(_ call: RealmServiceCall) -> Int {
        MemoryLayout<RealmServiceCall>.stride + call.route.utf8.count + call.payload.count
            + (call.streamChunk?.id.utf8.count ?? 0)
    }

    private static func failure(_ id: UUID, code: String, _ message: String) -> RealmServiceReply {
        RealmServiceReply(id: id, failure: RealmServiceFailure(code: code, message: message))
    }
}
