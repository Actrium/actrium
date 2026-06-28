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
            DevToolsManager.shared.onToggleRNInspector = { [weak self] in
                self?.emitToggleRNInspector()
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

    // MARK: - Engine Monitor

    /// Update engine monitor snapshot (polled from Host and pushed via RN bridge)
    @objc
    func updateEngineMonitor(_ snapshotJson: String?) {
        DispatchQueue.main.asyncAfter(deadline: .now()) {
            DevToolsManager.shared.updateEngineMonitorSnapshot(snapshotJson)
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

    /// Update RN Inspector active state (called from RN when Inspector is toggled)
    @objc
    func setRNInspectorActive(_ active: Bool) {
        DispatchQueue.main.asyncAfter(deadline: .now()) {
            DevToolsManager.shared.isRNInspectorActive = active
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

    /// Select a node in the DevTools tree (called from RN after Inspect click)
    @objc
    func selectNodeFromInspect(_ nodeId: String) {
        DispatchQueue.main.asyncAfter(deadline: .now()) {
            // Select the node in the tree
            DevToolsManager.shared.selectNode(nodeId)
            // Also update highlight state
            DevToolsManager.shared.highlightedNodeId = nodeId
        }
    }

    /// Update box model data for the selected node
    @objc
    func updateBoxModel(_ data: NSDictionary) {
        DispatchQueue.main.asyncAfter(deadline: .now()) {
            guard let frame = data["frame"] as? [String: Any] else { return }

            let x = frame["x"] as? CGFloat ?? 0
            let y = frame["y"] as? CGFloat ?? 0
            let width = frame["width"] as? CGFloat ?? 0
            let height = frame["height"] as? CGFloat ?? 0

            // Parse margin
            let marginDict = data["margin"] as? [String: Any] ?? [:]
            let margin = (
                top: marginDict["top"] as? CGFloat ?? 0,
                right: marginDict["right"] as? CGFloat ?? 0,
                bottom: marginDict["bottom"] as? CGFloat ?? 0,
                left: marginDict["left"] as? CGFloat ?? 0
            )

            // Parse padding
            let paddingDict = data["padding"] as? [String: Any] ?? [:]
            let padding = (
                top: paddingDict["top"] as? CGFloat ?? 0,
                right: paddingDict["right"] as? CGFloat ?? 0,
                bottom: paddingDict["bottom"] as? CGFloat ?? 0,
                left: paddingDict["left"] as? CGFloat ?? 0
            )

            // Parse border (optional)
            let borderDict = data["border"] as? [String: Any] ?? [:]
            let border = (
                top: borderDict["top"] as? CGFloat ?? 0,
                right: borderDict["right"] as? CGFloat ?? 0,
                bottom: borderDict["bottom"] as? CGFloat ?? 0,
                left: borderDict["left"] as? CGFloat ?? 0
            )

            DevToolsManager.shared.updateBoxModel(
                margin: margin,
                padding: padding,
                border: border,
                width: width,
                height: height,
                x: x,
                y: y
            )

            // Update computed styles if provided
            if let style = data["style"] as? [String: Any] {
                DevToolsManager.shared.updateComputedStyles(
                    position: style["position"] as? String,
                    backgroundColor: style["backgroundColor"] as? String,
                    top: style["top"] as? CGFloat,
                    left: style["left"] as? CGFloat,
                    right: style["right"] as? CGFloat,
                    bottom: style["bottom"] as? CGFloat,
                    display: style["display"] as? String,
                    flexDirection: style["flexDirection"] as? String,
                    alignItems: style["alignItems"] as? String,
                    justifyContent: style["justifyContent"] as? String,
                    flex: style["flex"] as? CGFloat,
                    opacity: style["opacity"] as? CGFloat,
                    overflow: style["overflow"] as? String
                )
            }
        }
    }

    /// Emit request to toggle inspect mode in RN
    private func emitToggleInspect() {
        if hasListeners {
            sendEvent(withName: "onToggleInspect", body: nil)
        }
    }

    /// Emit request to toggle RN's built-in Element Inspector
    private func emitToggleRNInspector() {
        if hasListeners {
            sendEvent(withName: "onToggleRNInspector", body: nil)
        }
    }

    /// Emit request to select a node in RN (for Inspect click)
    private func emitSelectNode(_ nodeId: String) {
        if hasListeners {
            sendEvent(withName: "onSelectNode", body: ["nodeId": nodeId])
        } else {
        }
    }

    // MARK: - RCTEventEmitter

    override func supportedEvents() -> [String]! {
        return ["onTogglePause", "onToggleInspect", "onSelectNode", "onToggleRNInspector"]
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
