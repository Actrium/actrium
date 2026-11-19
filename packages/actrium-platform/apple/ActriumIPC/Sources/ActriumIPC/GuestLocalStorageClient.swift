import Foundation

@MainActor
public final class GuestLocalStorageClient {
    private struct Pending {
        let tabId: String
        let complete: (GuestLocalStorageReply) -> Void
    }

    private let connection: IPCHelperConnection
    private var pending: [UUID: Pending] = [:]

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
        guard pending.count < 128 else {
            reply(GuestLocalStorageReply(id: call.id, error: "Too many pending local storage operations."))
            return
        }
        do {
            let data = try JSONEncoder().encode(call)
            try connection.sendHostEvent(
                name: GuestLocalStorageCall.requestEvent,
                userInfoJSON: String(decoding: data, as: UTF8.self),
                tabId: tabId
            )
            pending[call.id] = Pending(tabId: tabId, complete: reply)
            let id = call.id
            let timeout = GuestLocalStorageReply.timedOut(call)
            DispatchQueue.main.asyncAfter(deadline: DispatchTime(uptimeNanoseconds: call.deadlineUptimeNanoseconds)) {
                [weak self] in
                self?.pending.removeValue(forKey: id)?.complete(timeout)
            }
        } catch {
            reply(GuestLocalStorageReply(id: call.id, error: "Could not send the local storage operation."))
        }
    }

    public func receive(json: String, tabId: String) {
        guard let reply = try? JSONDecoder().decode(GuestLocalStorageReply.self, from: Data(json.utf8)),
              pending[reply.id]?.tabId == tabId else { return }
        pending.removeValue(forKey: reply.id)?.complete(reply)
    }
}
