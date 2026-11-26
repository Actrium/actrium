import Foundation

@MainActor
public final class GuestLocalStorageClient {
    private enum RequestError: Error {
        case tooLarge
    }

    private final class SendState: @unchecked Sendable {
        private let lock = NSLock()
        private var started = false

        func begin(before deadline: UInt64) -> Bool {
            lock.withLock {
                guard DispatchTime.now().uptimeNanoseconds < deadline else { return false }
                started = true
                return true
            }
        }

        var hasStarted: Bool { lock.withLock { started } }
    }

    private struct Pending {
        let tabId: String
        let complete: (GuestLocalStorageReply) -> Void
    }

    private let connection: IPCHelperConnection
    private var pending: [UUID: Pending] = [:]
    private var queuedSends = 0

    public init(connection: IPCHelperConnection) {
        self.connection = connection
    }

    public func send(
        _ call: GuestLocalStorageCall, tabId: String,
        reply: @escaping (GuestLocalStorageReply) -> Void
    ) {
        guard DispatchTime.now().uptimeNanoseconds < call.deadlineUptimeNanoseconds else {
            reply(.expiredBeforeStart(id: call.id))
            return
        }
        guard pending.count < 128, queuedSends < 128 else {
            reply(GuestLocalStorageReply(id: call.id, error: "Too many pending local storage operations."))
            return
        }
        let id = call.id
        let state = SendState()
        let timeout = GuestLocalStorageReply.timedOut(call)
        pending[id] = Pending(tabId: tabId, complete: reply)
        DispatchQueue.main.asyncAfter(deadline: DispatchTime(uptimeNanoseconds: call.deadlineUptimeNanoseconds)) {
            [weak self] in
            self?.complete(state.hasStarted ? timeout : .expiredBeforeStart(id: id))
        }
        queuedSends += 1
        do {
            try connection.enqueueHostEvent(
                name: GuestLocalStorageCall.requestEvent, tabId: tabId, retainedBytes: Self.retainedBytes(call)
            ) {
                try Self.prepareJSON(call)
            } beginSend: {
                state.begin(before: call.deadlineUptimeNanoseconds)
            } completion: { [weak self] error in
                DispatchQueue.main.async { [weak self] in self?.sendFinished(id: id, error: error) }
            }
        } catch {
            sendFinished(id: id, error: error)
        }
    }

    private nonisolated static func prepareJSON(_ call: GuestLocalStorageCall) throws -> String? {
        let deadline = call.deadlineUptimeNanoseconds
        guard DispatchTime.now().uptimeNanoseconds < deadline else { return nil }
        let data = try JSONEncoder().encode(call)
        guard data.count <= call.operation.maxRequestBytes else { throw RequestError.tooLarge }
        return String(decoding: data, as: UTF8.self)
    }

    private static func retainedBytes(_ call: GuestLocalStorageCall) -> Int {
        let callBytes = MemoryLayout<GuestLocalStorageCall>.stride + call.key.utf8.count + (call.value?.utf8.count ?? 0)
        guard let replacement = call.replacement else { return callBytes }
        let removedBytes = replacement.remove.reduce(0) { $0 + MemoryLayout<String>.stride + $1.utf8.count }
        let writtenBytes = replacement.set.reduce(0) { total, item in
            total + MemoryLayout<GuestLocalStorageCall.Replacement.Item>.stride + item.key.utf8.count + item.value.utf8.count
        }
        return callBytes + removedBytes + writtenBytes + (replacement.expected?.utf8.count ?? 0)
    }

    private func sendFinished(id: UUID, error: Error?) {
        queuedSends -= 1
        guard let error else { return }
        if case RequestError.tooLarge = error {
            complete(GuestLocalStorageReply(id: id, error: "The local storage request is too large.", errorCode: .requestTooLarge))
        } else if case IPCError.sendQueueFull = error {
            complete(GuestLocalStorageReply(id: id, error: "Too many pending local storage operations."))
        } else {
            complete(GuestLocalStorageReply(id: id, error: "Could not send the local storage operation."))
        }
    }

    public func receive(json: String, tabId: String) {
        guard let reply = try? JSONDecoder().decode(GuestLocalStorageReply.self, from: Data(json.utf8)),
              pending[reply.id]?.tabId == tabId else { return }
        complete(reply)
    }

    private func complete(_ reply: GuestLocalStorageReply) {
        pending.removeValue(forKey: reply.id)?.complete(reply)
    }
}
