import ActriumIPC
import Foundation
import React

@objc(GuestFileImport)
final class GuestFileImportModule: NSObject {
    @objc(pick:reloadKey:source:accept:multiple:resolver:rejecter:)
    func pick(
        _ tabId: String, reloadKey: NSNumber, source: String, accept: [String], multiple: Bool,
        resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        guard let source = GuestFileImportCall.Source(rawValue: source) else {
            reject("invalid_file_import", "Invalid picker source.", nil)
            return
        }
        let call = GuestFileImportCall(reloadKey: reloadKey.intValue, source: source, accept: accept, multiple: multiple)
        guard call.hasValidArguments else {
            reject("invalid_file_import", "Invalid picker arguments.", nil)
            return
        }
        DispatchQueue.main.async {
            RendererProcessController.pickFiles(call, tabId: tabId) { response in
                if let error = response.error {
                    reject("file_import_failed", error, nil)
                } else {
                    resolve(["files": response.files.map {
                        ["name": $0.name, "base64": $0.data.base64EncodedString()]
                    }])
                }
            }
        }
    }

    @objc static func requiresMainQueueSetup() -> Bool { false }
}
