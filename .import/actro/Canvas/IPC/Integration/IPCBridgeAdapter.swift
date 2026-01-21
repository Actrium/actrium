//
//  IPCBridgeAdapter.swift
//  Canvas
//
//  Adapter for integrating IPC module with XPCBridgeService.
//  Provides a clean API for IPC communication using NSXPCConnection.
//

import Foundation
import Combine

// MARK: - IPC Bridge Adapter

/// Adapter that bridges the IPC module with XPCBridgeService
@MainActor
public final class IPCBridgeAdapter: ObservableObject {
    // MARK: - Singleton

    public static let shared = IPCBridgeAdapter()

    // MARK: - Dependencies

    private let interactionManager: IPCInteractionManager

    // MARK: - Initialization

    private init() {
        self.interactionManager = IPCInteractionManager.shared
        NSLog("📡 [IPCBridgeAdapter] Initialized")
    }

    // MARK: - Service Management

    /// Allocate XPC service for a tab
    public func allocateService(for tabId: UUID, fingerprint: String) throws -> String {
        return try interactionManager.allocateService(for: tabId, fingerprint: fingerprint)
    }

    /// Mark tab as connected
    public func markTabConnected(_ tabId: UUID, connectionId: String? = nil) {
        interactionManager.markTabConnected(tabId, connectionId: connectionId)
    }

    /// Mark service as ready
    public func markServiceReady(_ serviceId: String) {
        interactionManager.markServiceReady(serviceId)
    }

    /// Release tab resources
    public func releaseTab(_ tabId: UUID) {
        interactionManager.releaseTab(tabId)
    }

    // MARK: - Tab Configuration

    /// Send tab configuration
    func sendTabConfig(_ config: XPCTabConfig, isOverviewMode: Bool) {
        let ipcConfig = IPCXPCTabConfig(
            tabId: config.tabId.uuidString,
            title: config.title,
            containerWidth: config.containerSize.width,
            containerHeight: config.containerSize.height,
            isOverviewMode: isOverviewMode,
            configJSON: makeConfigJSON(from: config, isOverviewMode: isOverviewMode)
        )
        interactionManager.sendTabConfig(ipcConfig)
    }

    private func makeConfigJSON(from config: XPCTabConfig, isOverviewMode: Bool) -> String {
        let dict = config.toDictionary(isOverviewMode: isOverviewMode)
        guard let data = try? JSONSerialization.data(withJSONObject: dict),
              let json = String(data: data, encoding: .utf8) else {
            return "{}"
        }
        return json
    }

    // MARK: - Container Size

    /// Send container size change
    public func sendContainerSizeChanged(tabId: UUID, size: CGSize, requestId: Int? = nil) {
        interactionManager.sendContainerSizeChanged(
            tabId: tabId,
            size: size,
            requestId: requestId
        )
    }

    // MARK: - Tab Binding

    /// Bind host tab ID to internal connection ID
    public func bindTab(hostTabId: UUID, internalId: String, serviceId: String) {
        Task {
            await sendBindTabViaXPC(hostTabId: hostTabId, internalId: internalId, serviceId: serviceId)
        }
    }

    private func sendBindTabViaXPC(hostTabId: UUID, internalId: String, serviceId: String) async {
        let request = IPCRequest(
            kind: .control,
            payload: TabBindingPayload(hostTabId: hostTabId, internalId: internalId),
            targetServiceId: serviceId,
            tabId: hostTabId,
            priority: .high,
            retryPolicy: .aggressive
        )

        let result = await interactionManager.perform(request, responseType: EmptyPayload.self)

        switch result {
        case .success:
            NSLog("📡 [IPCBridgeAdapter] bindTab succeeded: host=%@ internal=%@",
                  String(hostTabId.uuidString.prefix(8)), String(internalId.prefix(8)))
        case .failure(let error):
            NSLog("❌ [IPCBridgeAdapter] bindTab failed: %@", error.localizedDescription)
        }
    }

    // MARK: - Pending Tab ID

    /// Send pending tab ID notification (pre-connection)
    public func sendPendingTabId(_ tabId: UUID, serviceId: String) {
        let hostProcessId = ProcessInfo.processInfo.processIdentifier
        let notificationName = "com.canvas.pendingTabId.\(serviceId)"

        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name(notificationName),
            object: nil,
            userInfo: [
                "tabId": tabId.uuidString,
                "hostProcessId": hostProcessId
            ],
            deliverImmediately: true
        )
    }

    // MARK: - Status

    /// Check if a tab is ready for communication
    public func isTabReady(_ tabId: UUID) -> Bool {
        return interactionManager.isTabReady(tabId)
    }

    /// Get service ID for a tab
    public func serviceId(for tabId: UUID) -> String? {
        return interactionManager.serviceId(for: tabId)
    }

    // MARK: - XPC Endpoint Handling

    /// Handle XPC endpoint received from Service
    public func handleXPCEndpoint(_ endpointData: Data, serviceId: String) {
        NSLog("📡 [IPCBridgeAdapter] Received XPC endpoint for service: %@",
              serviceId.components(separatedBy: ".").last ?? "")

        guard let endpoint = try? NSKeyedUnarchiver.unarchivedObject(
            ofClass: NSXPCListenerEndpoint.self,
            from: endpointData
        ) else {
            NSLog("❌ [IPCBridgeAdapter] Failed to decode XPC endpoint")
            return
        }

        Task {
            await storeEndpointForService(endpoint, serviceId: serviceId)
        }
    }

    private var pendingEndpoints: [String: NSXPCListenerEndpoint] = [:]

    private func storeEndpointForService(_ endpoint: NSXPCListenerEndpoint, serviceId: String) async {
        pendingEndpoints[serviceId] = endpoint
        NSLog("📡 [IPCBridgeAdapter] Stored endpoint for service: %@",
              serviceId.components(separatedBy: ".").last ?? "")
    }

    /// Get stored endpoint for a service
    public func endpoint(for serviceId: String) -> NSXPCListenerEndpoint? {
        pendingEndpoints[serviceId]
    }
}

// MARK: - Notification Extension

extension IPCBridgeAdapter {
    /// Setup listener for XPC endpoint notifications from Service
    nonisolated func setupEndpointListener() {
        DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name("com.canvas.xpcEndpoint"),
            object: nil,
            queue: .main
        ) { [weak self] notification in
            guard let self = self,
                  let userInfo = notification.userInfo,
                  let bundleId = userInfo["bundleId"] as? String,
                  let endpointBase64 = userInfo["endpoint"] as? String,
                  let endpointData = Data(base64Encoded: endpointBase64) else {
                return
            }

            // Verify this is for our process
            if let hostPid = userInfo["hostProcessId"] as? Int32,
               hostPid != ProcessInfo.processInfo.processIdentifier {
                return
            }

            Task { @MainActor in
                self.handleXPCEndpoint(endpointData, serviceId: bundleId)
            }
        }
    }
}
