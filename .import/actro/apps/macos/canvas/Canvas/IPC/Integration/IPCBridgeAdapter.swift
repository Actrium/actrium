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

    // MARK: - Status

    /// Check if a tab is ready for communication
    public func isTabReady(_ tabId: UUID) -> Bool {
        return interactionManager.isTabReady(tabId)
    }

    /// Get service ID for a tab
    public func serviceId(for tabId: UUID) -> String? {
        return interactionManager.serviceId(for: tabId)
    }
}
