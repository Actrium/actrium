import ActriumIPC
import Foundation
import React

@objc(GuestFileExport)
final class GuestFileExportModule: NSObject {
    @objc(save:reloadKey:name:base64:resolver:rejecter:)
    func save(
        _ tabId: String, reloadKey: NSNumber, name: String, base64: String,
        resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        guard base64.utf8.count <= GuestFileExportCall.maxBase64Bytes, let data = Data(base64Encoded: base64) else {
            reject("invalid_file_export", "The file data is invalid or exceeds 7 MiB.", nil)
            return
        }
        let call = GuestFileExportCall(reloadKey: reloadKey.intValue, name: name, data: data)
        guard call.hasValidArguments else {
            reject("invalid_file_export", "The file name is invalid or the file exceeds 7 MiB.", nil)
            return
        }
        DispatchQueue.main.async {
            RendererProcessController.saveFile(call, tabId: tabId) { response in
                if let error = response.error {
                    reject("file_export_failed", error, nil)
                } else {
                    resolve(["saved": response.saved])
                }
            }
        }
    }

    @objc static func requiresMainQueueSetup() -> Bool { false }
}
