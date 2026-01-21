//
//  IPCBridgeAdapter.swift
//  Canvas
//
//  Adapter for integrating new IPC module with existing XPCBridgeService.
//  Provides backward-compatible API while using the new IPC infrastructure.
//

import Foundation
import Combine

// MARK: - IPC Bridge Adapter

/// Adapter that bridges the new IPC module with existing XPCBridgeService
///
/// Usage:
/// 1. Replace direct DistributedNotification calls with adapter methods
/// 2. Adapter checks feature flag and routes to appropriate implementation
/// 3. Gradual migration: start with non-critical paths, then critical ones
@MainActor
public final class IPCBridgeAdapter: ObservableObject {
    // MARK: - Singleton

    public static let shared = IPCBridgeAdapter()

    // MARK: - Dependencies

    private let interactionManager: IPCInteractionManager
    private let configuration: IPCConfiguration

    // MARK: - State

    /// Whether the new XPC transport is enabled
    public var isXPCEnabled: Bool {
        configuration.featureFlags.useXPCConnection
    }

    // MARK: - Initialization

    private init() {
        self.interactionManager = IPCInteractionManager.shared
        self.configuration = IPCConfiguration.default

        NSLog("📡 [IPCBridgeAdapter] Initialized with useXPCConnection=%@",
              configuration.featureFlags.useXPCConnection ? "true" : "false")
    }

    // MARK: - Service Management

    /// Allocate XPC service for a tab
    /// Delegates to interactionManager when XPC is enabled
    public func allocateService(for tabId: UUID, fingerprint: String) throws -> String {
        if isXPCEnabled {
            return try interactionManager.allocateService(for: tabId, fingerprint: fingerprint)
        } else {
            // Fallback to XPCServicePool directly
            return try XPCServicePool.shared.allocateService(for: tabId, fingerprint: fingerprint)
        }
    }

    /// Mark tab as connected
    public func markTabConnected(_ tabId: UUID, connectionId: String? = nil) {
        if isXPCEnabled {
            interactionManager.markTabConnected(tabId, connectionId: connectionId)
        }
        // Note: XPCBridgeService also does its own tracking
    }

    /// Mark service as ready
    public func markServiceReady(_ serviceId: String) {
        if isXPCEnabled {
            interactionManager.markServiceReady(serviceId)
        }
    }

    /// Release tab resources
    public func releaseTab(_ tabId: UUID) {
        if isXPCEnabled {
            interactionManager.releaseTab(tabId)
        }
    }

    // MARK: - Tab Configuration

    /// Send tab configuration (internal - uses XPCTabConfig from XPCBridgeService)
    func sendTabConfig(_ config: XPCTabConfig, isOverviewMode: Bool) {
        if isXPCEnabled {
            // Convert to IPC format
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
        // Note: XPCBridgeService handles legacy path
    }

    private func makeConfigJSON(from config: XPCTabConfig, isOverviewMode: Bool) -> String {
        let dict = config.toDictionary(isOverviewMode: isOverviewMode)
        // Add any additional fields needed
        guard let data = try? JSONSerialization.data(withJSONObject: dict),
              let json = String(data: data, encoding: .utf8) else {
            return "{}"
        }
        return json
    }

    // MARK: - Container Size

    /// Send container size change
    public func sendContainerSizeChanged(tabId: UUID, size: CGSize, requestId: Int? = nil) {
        if isXPCEnabled {
            interactionManager.sendContainerSizeChanged(
                tabId: tabId,
                size: size,
                requestId: requestId
            )
        }
        // Note: XPCBridgeService handles legacy path
    }

    // MARK: - Tab Binding

    /// Bind host tab ID to internal connection ID
    public func bindTab(hostTabId: UUID, internalId: String, serviceId: String) {
        if isXPCEnabled {
            // Use the new transport for binding
            Task {
                await sendBindTabViaXPC(hostTabId: hostTabId, internalId: internalId, serviceId: serviceId)
            }
        } else {
            // Legacy: use DistributedNotification
            sendBindTabViaNotification(hostTabId: hostTabId, internalId: internalId, serviceId: serviceId)
        }
    }

    private func sendBindTabViaNotification(hostTabId: UUID, internalId: String, serviceId: String) {
        let hostProcessId = ProcessInfo.processInfo.processIdentifier
        let notificationName = "com.canvas.bindTab.\(hostProcessId).\(serviceId)"

        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name(notificationName),
            object: nil,
            userInfo: [
                "tabId": hostTabId.uuidString,
                "internalId": internalId
            ],
            deliverImmediately: true
        )
    }

    private func sendBindTabViaXPC(hostTabId: UUID, internalId: String, serviceId: String) async {
        // Create bind request using IPC module
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
            // Fallback to notification
            sendBindTabViaNotification(hostTabId: hostTabId, internalId: internalId, serviceId: serviceId)
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
        if isXPCEnabled {
            return interactionManager.isTabReady(tabId)
        } else {
            return XPCBridgeService.shared.isTabReady(tabId)
        }
    }

    /// Get service ID for a tab
    public func serviceId(for tabId: UUID) -> String? {
        if isXPCEnabled {
            return interactionManager.serviceId(for: tabId)
        } else {
            return XPCBridgeService.shared.serviceId(for: tabId)
        }
    }

    // MARK: - XPC Endpoint Handling

    /// Handle XPC endpoint received from Service
    /// Called when Service exports its NSXPCListenerEndpoint
    public func handleXPCEndpoint(_ endpointData: Data, serviceId: String) {
        guard isXPCEnabled else { return }

        NSLog("📡 [IPCBridgeAdapter] Received XPC endpoint for service: %@",
              serviceId.components(separatedBy: ".").last ?? "")

        // Decode endpoint
        guard let endpoint = try? NSKeyedUnarchiver.unarchivedObject(
            ofClass: NSXPCListenerEndpoint.self,
            from: endpointData
        ) else {
            NSLog("❌ [IPCBridgeAdapter] Failed to decode XPC endpoint")
            return
        }

        // Store for later connection
        // The XPCConnectionTransport will use this when connecting
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

            // Dispatch to MainActor for actor-isolated method
            Task { @MainActor in
                self.handleXPCEndpoint(endpointData, serviceId: bundleId)
            }
        }
    }
}
