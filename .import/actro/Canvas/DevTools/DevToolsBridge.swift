import Foundation
import React

/// Native module for DevTools message logging from React Native
@objc(DevToolsBridge)
class DevToolsBridge: RCTEventEmitter {

    private var hasListeners = false

    /// Log an application message (APP_FETCH_REQUEST, etc.)
    @objc
    func logMessage(_ direction: String, _ event: String, _ payload: String?, _ tabId: String?) {
        let msgDirection = parseDirection(direction)
        let payloadDict = parsePayload(payload)

        // Use asyncAfter to avoid "Publishing changes from within view updates" warning
        DispatchQueue.main.asyncAfter(deadline: .now()) {
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

        // Use asyncAfter to avoid "Publishing changes from within view updates" warning
        DispatchQueue.main.asyncAfter(deadline: .now()) {
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

    /// Update the engine pause state (called from RN after engine.pause()/resume())
    @objc
    func setEnginePaused(_ isPaused: Bool) {
        DispatchQueue.main.asyncAfter(deadline: .now()) {
            DevToolsManager.shared.isEnginePaused = isPaused
        }
    }

    /// Set up the pause toggle callback from DevTools UI
    @objc
    func registerPauseHandler() {
        DispatchQueue.main.asyncAfter(deadline: .now()) {
            DevToolsManager.shared.onTogglePause = { [weak self] in
                self?.emitTogglePause()
            }
        }
    }

    /// Emit toggle pause event to RN
    private func emitTogglePause() {
        if hasListeners {
            sendEvent(withName: "onTogglePause", body: nil)
        }
    }

    // MARK: - RCTEventEmitter

    override func supportedEvents() -> [String]! {
        return ["onTogglePause"]
    }

    override func startObserving() {
        hasListeners = true
    }

    override func stopObserving() {
        hasListeners = false
    }

    // MARK: - Helpers

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
    override static func requiresMainQueueSetup() -> Bool {
        return false
    }

    @objc
    override static func moduleName() -> String! {
        return "DevToolsBridge"
    }
}
