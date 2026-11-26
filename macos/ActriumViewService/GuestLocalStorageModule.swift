import ActriumIPC
import Foundation
import React

@objc(GuestLocalStorage)
final class GuestLocalStorageModule: NSObject {
    private struct Input: Decodable {
        let key: String
        let value: String?
        let replacement: GuestLocalStorageCall.Replacement?
    }

    @objc(perform:documentId:operation:inputJSON:resolver:rejecter:)
    func perform(
        _ tabId: String,
        documentId: String,
        operation: String,
        inputJSON: String,
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        guard let document = UUID(uuidString: documentId),
              let operation = GuestLocalStorageCall.Operation(rawValue: operation),
              inputJSON.utf8.count <= Self.requestLimit(for: operation),
              let input = try? JSONDecoder().decode(Input.self, from: Data(inputJSON.utf8)) else {
            reject("invalid_local_storage_call", "Invalid local storage arguments.", nil)
            return
        }
        let call = GuestLocalStorageCall(
            id: UUID(), documentId: document, operation: operation, key: input.key, value: input.value,
            replacement: input.replacement
        )
        guard call.hasValidArguments else {
            reject("invalid_local_storage_call", "Invalid local storage arguments.", nil)
            return
        }
        DispatchQueue.main.async {
            RendererProcessController.callLocalStorage(call, tabId: tabId) { response in
                if let error = response.error {
                    reject(response.errorCode?.rawValue ?? "local_storage_failed", error, nil)
                } else {
                    do {
                        let data = try JSONEncoder().encode(response.value)
                        resolve(String(decoding: data, as: UTF8.self))
                    } catch {
                        reject("local_storage_failed", "Could not encode the local storage result.", nil)
                    }
                }
            }
        }
    }

    @objc static func requiresMainQueueSetup() -> Bool { false }

    private static func requestLimit(for operation: GuestLocalStorageCall.Operation) -> Int {
        operation == .replaceItems ? GuestLocalStorageCall.maxRequestBytes : GuestLocalStorageCall.maxSingleRequestBytes
    }
}
