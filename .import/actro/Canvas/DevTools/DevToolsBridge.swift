import Foundation
import React

/// Native module for DevTools message logging from React Native
@objc(DevToolsBridge)
class DevToolsBridge: NSObject {

    /// Log an application message (APP_FETCH_REQUEST, etc.)
    @objc
    func logMessage(_ direction: String, _ event: String, _ payload: String?, _ tabId: String?) {
        let msgDirection = parseDirection(direction)
        let payloadDict = parsePayload(payload)

        DispatchQueue.main.async {
            DevToolsManager.shared.logMessage(
                direction: msgDirection,
                event: event,
                payload: payloadDict,
                tabId: tabId
            )
        }
    }

    /// Log a render operation (CREATE, UPDATE, DELETE, TEXT)
    @objc
    func logRenderOp(_ operationType: String, _ componentType: String?, _ nodeId: String?, _ payload: String?, _ tabId: String?) {
        let payloadDict = parsePayload(payload) as? [String: Any]

        DispatchQueue.main.async {
            DevToolsManager.shared.logRenderOperation(
                direction: .guestToHost,
                operationType: operationType,
                componentType: componentType,
                nodeId: nodeId,
                props: payloadDict,
                tabId: tabId
            )
        }
    }

    private func parseDirection(_ direction: String) -> MessageDirection {
        switch direction {
        case "guestToHost":
            return .guestToHost
        case "hostToGuest":
            return .hostToGuest
        default:
            return .guestToHost
        }
    }

    private func parsePayload(_ payload: String?) -> Any? {
        guard let payloadStr = payload, !payloadStr.isEmpty else { return nil }
        if let data = payloadStr.data(using: .utf8),
           let parsed = try? JSONSerialization.jsonObject(with: data) {
            return parsed
        }
        return payloadStr
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
