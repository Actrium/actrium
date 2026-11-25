import ActriumIPC
import Foundation
import React

@objc(GuestMenu)
final class GuestMenuModule: NSObject {
    @objc(show:reloadKey:contentJSON:resolver:rejecter:)
    func show(
        _ tabId: String, reloadKey: NSNumber, contentJSON: String,
        resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        guard contentJSON.utf8.count <= GuestMenuCall.maxRequestBytes,
              let content = try? JSONDecoder().decode(GuestMenuCall.Content.self, from: Data(contentJSON.utf8)) else {
            reject("invalid_guest_menu", "Invalid menu content.", nil)
            return
        }
        let call = GuestMenuCall(reloadKey: reloadKey.intValue, content: content)
        guard call.hasValidArguments else {
            reject("invalid_guest_menu", "Invalid menu arguments.", nil)
            return
        }
        DispatchQueue.main.async {
            RendererProcessController.showGuestMenu(call, tabId: tabId) { reply in
                if let error = reply.error {
                    reject("guest_menu_failed", error, nil)
                } else {
                    resolve(reply.value)
                }
            }
        }
    }

    @objc static func requiresMainQueueSetup() -> Bool { false }
}
