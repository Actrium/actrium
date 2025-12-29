import Foundation
import React

/// Native module for DevTools message logging from React Native
@objc(DevToolsBridge)
class DevToolsBridge: NSObject {

    /// Log a Host-Guest message
    /// - Parameters:
    ///   - direction: "guestToHost" or "hostToGuest"
    ///   - event: Event name (e.g., "APP_FETCH_REQUEST")
    ///   - payload: JSON string of the payload
    ///   - tabId: Optional tab identifier
    @objc
    func logMessage(_ direction: String, _ event: String, _ payload: String?, _ tabId: String?) {
        let msgDirection: MessageDirection
        switch direction {
        case "guestToHost":
            msgDirection = .guestToHost
        case "hostToGuest":
            msgDirection = .hostToGuest
        default:
            msgDirection = .guestToHost
        }

        // Parse payload string back to dictionary for proper formatting
        var payloadDict: Any? = nil
        if let payloadStr = payload, !payloadStr.isEmpty {
            if let data = payloadStr.data(using: .utf8),
               let parsed = try? JSONSerialization.jsonObject(with: data) {
                payloadDict = parsed
            } else {
                payloadDict = payloadStr
            }
        }

        DispatchQueue.main.async {
            DevToolsManager.shared.logMessage(
                direction: msgDirection,
                event: event,
                payload: payloadDict,
                tabId: tabId
            )
        }
    }

    @objc
    static func requiresMainQueueSetup() -> Bool {
        return false
    }

    @objc
    static func moduleName() -> String! {
        return "DevToolsBridge"
    }
}
