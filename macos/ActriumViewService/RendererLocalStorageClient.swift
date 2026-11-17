import ActriumIPC
import Foundation
import React

@objc(GuestLocalStorage)
final class GuestLocalStorageModule: NSObject {
    @objc(perform:documentId:operation:key:value:resolver:rejecter:)
    func perform(
        _ tabId: String,
        documentId: String,
        operation: String,
        key: String,
        value: String?,
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        guard let document = UUID(uuidString: documentId),
              let operation = GuestLocalStorageCall.Operation(rawValue: operation) else {
            reject("invalid_local_storage_call", "Invalid local storage operation.", nil)
            return
        }
        let call = GuestLocalStorageCall(id: UUID(), documentId: document, operation: operation, key: key, value: value)
        guard call.hasValidArguments else {
            reject("invalid_local_storage_call", "Invalid local storage arguments.", nil)
            return
        }
        DispatchQueue.main.async {
            RendererProcessController.callLocalStorage(call, tabId: tabId) { response in
                if let error = response.error {
                    reject("local_storage_failed", error, nil)
                } else {
                    resolve(response.value ?? NSNull() as Any)
                }
            }
        }
    }

    @objc static func requiresMainQueueSetup() -> Bool { false }
}

@MainActor
final class RendererLocalStorageClient {
    private struct Pending {
        let tabId: String
        let complete: (GuestLocalStorageReply) -> Void
    }

    private let connection: IPCHelperConnection
    private var pending: [UUID: Pending] = [:]

    init(connection: IPCHelperConnection) {
        self.connection = connection
    }

    func send(
        _ call: GuestLocalStorageCall, tabId: String,
        reply: @escaping (GuestLocalStorageReply) -> Void
    ) {
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
            DispatchQueue.main.asyncAfter(deadline: .now() + .seconds(GuestLocalStorageCall.timeoutSeconds)) {
                [weak self] in
                self?.pending.removeValue(forKey: call.id)?.complete(
                    GuestLocalStorageReply(id: call.id, error: "The local storage operation timed out.")
                )
            }
        } catch {
            reply(GuestLocalStorageReply(id: call.id, error: "Could not send the local storage operation."))
        }
    }

    func receive(json: String, tabId: String) {
        guard let reply = try? JSONDecoder().decode(GuestLocalStorageReply.self, from: Data(json.utf8)),
              pending[reply.id]?.tabId == tabId else { return }
        pending.removeValue(forKey: reply.id)?.complete(reply)
    }
}
