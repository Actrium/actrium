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
            DevToolsManager.shared.onToggleInspect = { [weak self] in
                self?.emitToggleInspect()
            }
            DevToolsManager.shared.onSelectNodeFromTree = { [weak self] nodeId in
                self?.emitSelectNode(nodeId)
            }
        }
    }

    /// Emit toggle pause event to RN
    private func emitTogglePause() {
        if hasListeners {
            sendEvent(withName: "onTogglePause", body: nil)
        }
    }

    // MARK: - Component Tree

    /// Update the component tree for DevTools Elements panel
    @objc
    func updateComponentTree(_ treeJson: String?, _ tabId: String?) {
        DispatchQueue.main.asyncAfter(deadline: .now()) {
            guard let json = treeJson, !json.isEmpty else {
                DevToolsManager.shared.componentTree = nil
                return
            }

            // Parse JSON to DevToolsComponentNode
            if let data = json.data(using: .utf8),
               let dict = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
                let tree = self.parseTreeNode(dict)
                DevToolsManager.shared.componentTree = tree
            }
        }
    }

    /// Parse a tree node from dictionary
    private func parseTreeNode(_ dict: [String: Any]) -> DevToolsComponentNode {
        let id = dict["id"] as? String ?? ""
        let type = dict["type"] as? String ?? "Unknown"
        let props = dict["props"] as? [String: Any] ?? [:]
        let childrenArray = dict["children"] as? [[String: Any]] ?? []

        let children = childrenArray.map { parseTreeNode($0) }

        return DevToolsComponentNode(
            id: id,
            type: type,
            props: props,
            children: children
        )
    }

    // MARK: - Inspect Mode

    /// Enable/disable inspect mode
    @objc
    func setInspectMode(_ enabled: Bool) {
        DispatchQueue.main.asyncAfter(deadline: .now()) {
            DevToolsManager.shared.isInspectMode = enabled
        }
    }

    /// Highlight a node with frame information
    @objc
    func highlightNode(_ nodeId: String?, _ frame: NSDictionary?) {
        DispatchQueue.main.asyncAfter(deadline: .now()) {
            guard let nodeId = nodeId else {
                DevToolsManager.shared.highlightedNodeId = nil
                DevToolsManager.shared.highlightFrame = nil
                return
            }

            DevToolsManager.shared.highlightedNodeId = nodeId

            if let frameDict = frame as? [String: Any] {
                DevToolsManager.shared.highlightFrame = CGRect(
                    x: frameDict["x"] as? CGFloat ?? 0,
                    y: frameDict["y"] as? CGFloat ?? 0,
                    width: frameDict["width"] as? CGFloat ?? 0,
                    height: frameDict["height"] as? CGFloat ?? 0
                )
            }
        }
    }

    /// Clear the highlight
    @objc
    func clearHighlight() {
        DispatchQueue.main.asyncAfter(deadline: .now()) {
            DevToolsManager.shared.highlightedNodeId = nil
            DevToolsManager.shared.highlightFrame = nil
        }
    }

    /// Emit request to toggle inspect mode in RN
    private func emitToggleInspect() {
        if hasListeners {
            sendEvent(withName: "onToggleInspect", body: nil)
        }
    }

    /// Emit request to select a node in RN (for Inspect click)
    private func emitSelectNode(_ nodeId: String) {
        if hasListeners {
            sendEvent(withName: "onSelectNode", body: ["nodeId": nodeId])
        }
    }

    // MARK: - RCTEventEmitter

    override func supportedEvents() -> [String]! {
        return ["onTogglePause", "onToggleInspect", "onSelectNode"]
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
