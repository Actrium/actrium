import Foundation

@MainActor
public final class GuestFileExportClient {
    private struct Pending {
        let id: UUID
        let reply: (GuestFileExportReply) -> Void
    }

    private let connection: IPCHelperConnection
    private let sendQueue = DispatchQueue(label: "io.actrium.renderer.file-export", qos: .userInitiated)
    private var pending: [String: Pending] = [:]

    public init(connection: IPCHelperConnection) {
        self.connection = connection
    }

    public func send(_ call: GuestFileExportCall, tabId: String, reply: @escaping (GuestFileExportReply) -> Void) {
        guard pending[tabId] == nil else {
            reply(GuestFileExportReply(id: call.id, error: "A file export is already in progress."))
            return
        }
        // The Host times out the dialog and reports the write result after confirmation.
        pending[tabId] = Pending(id: call.id, reply: reply)
        sendQueue.async { [weak self, connection] in
            do {
                let encoder = JSONEncoder()
                encoder.outputFormatting = .withoutEscapingSlashes
                let data = try encoder.encode(call)
                try connection.sendHostEvent(
                    name: GuestFileExportCall.requestEvent,
                    userInfoJSON: String(decoding: data, as: UTF8.self), tabId: tabId
                )
            } catch {
                let reply = GuestFileExportReply(id: call.id, error: "Could not send the file to the save dialog.")
                DispatchQueue.main.async { [weak self] in self?.complete(reply, tabId: tabId) }
            }
        }
    }

    public func receive(json: String, tabId: String) {
        guard let reply = try? JSONDecoder().decode(GuestFileExportReply.self, from: Data(json.utf8)) else { return }
        complete(reply, tabId: tabId)
    }

    public func close(tabId: String) {
        guard let request = pending.removeValue(forKey: tabId) else { return }
        request.reply(GuestFileExportReply(
            id: request.id, error: "The export session ended before the save result was confirmed."
        ))
    }

    private func complete(_ reply: GuestFileExportReply, tabId: String) {
        guard pending[tabId]?.id == reply.id else { return }
        pending.removeValue(forKey: tabId)?.reply(reply)
    }
}
