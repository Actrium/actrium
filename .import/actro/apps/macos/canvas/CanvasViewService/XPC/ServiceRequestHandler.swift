//
//  ServiceRequestHandler.swift
//  CanvasViewService
//
//  Handles incoming XPC requests from Host.
//  Implements CanvasHostXPCProtocol and delegates to ServiceReactNativeHost.
//

import Foundation
import AppKit

// MARK: - Service Request Handler

/// Handles incoming XPC requests from Host
/// Implements the CanvasHostXPCProtocol and processes requests
public final class ServiceRequestHandler: NSObject, CanvasHostXPCProtocol {
    // MARK: - Properties

    /// Delegate for forwarding requests to the React Native host
    weak var delegate: ServiceRequestHandlerDelegate?

    /// Host process ID for notification routing
    private var hostProcessId: Int32?

    /// Service bundle identifier
    private let serviceBundleId: String

    // MARK: - Initialization

    override init() {
        // Get service bundle ID
        let executablePath = ProcessInfo.processInfo.arguments[0]
        var bundlePath = (executablePath as NSString).deletingLastPathComponent
        bundlePath = (bundlePath as NSString).deletingLastPathComponent
        bundlePath = (bundlePath as NSString).deletingLastPathComponent
        if let serviceBundle = Bundle(path: bundlePath),
           let bundleId = serviceBundle.bundleIdentifier {
            serviceBundleId = bundleId
        } else {
            serviceBundleId = Bundle.main.bundleIdentifier ?? "unknown"
        }

        super.init()
    }

    /// Set the host process ID for routing
    func setHostProcessId(_ pid: Int32) {
        self.hostProcessId = pid
    }

    // MARK: - CanvasHostXPCProtocol Implementation

    public func sendTabConfig(_ config: ServiceXPCTabConfig, reply: @escaping (Bool, String?) -> Void) {
        NSLog("[ServiceXPC] Received tabConfig for tab=%@", String(config.tabId.prefix(8)))

        // Dispatch to main thread for UI operations
        DispatchQueue.main.async { [weak self] in
            guard let self = self else {
                reply(false, "Service handler deallocated")
                return
            }

            // Parse configJSON to dictionary
            var configDict: [String: Any] = [
                "tabId": config.tabId,
                "title": config.title,
                "containerWidth": config.containerWidth,
                "containerHeight": config.containerHeight,
                "isOverviewMode": config.isOverviewMode
            ]

            // Parse additional config from JSON
            if !config.configJSON.isEmpty,
               let jsonData = config.configJSON.data(using: .utf8),
               let jsonDict = try? JSONSerialization.jsonObject(with: jsonData) as? [String: Any] {
                for (key, value) in jsonDict {
                    configDict[key] = value
                }
            }

            // Forward to delegate or handle directly
            if let delegate = self.delegate {
                delegate.handleTabConfig(configDict) { success, error in
                    reply(success, error)
                }
            } else {
                // Fallback: post notification for backward compatibility
                self.postTabConfigNotification(config: configDict)
                reply(true, nil)
            }
        }
    }

    public func updateContainerSize(_ size: ServiceXPCContainerSize, reply: @escaping (Bool, String?) -> Void) {
        NSLog("[ServiceXPC] Received containerSize for tab=%@ size=%.0fx%.0f req=%d",
              String(size.tabId.prefix(8)), size.width, size.height, size.requestId)

        // Dispatch to main thread for UI operations
        DispatchQueue.main.async { [weak self] in
            guard let self = self else {
                reply(false, "Service handler deallocated")
                return
            }

            // Forward to delegate or handle directly
            if let delegate = self.delegate {
                delegate.handleContainerSizeChanged(
                    tabId: size.tabId,
                    size: size.size,
                    requestId: size.requestId,
                    isOverviewMode: size.isOverviewMode
                ) { success, error in
                    reply(success, error)

                    // Notify size applied if successful
                    if success {
                        ServiceXPCListener.shared.notifySizeApplied(
                            tabId: size.tabId,
                            size: size.size,
                            requestId: size.requestId
                        )
                    }
                }
            } else {
                // Fallback: post notification for backward compatibility
                self.postContainerSizeNotification(
                    tabId: size.tabId,
                    size: size.size,
                    requestId: size.requestId,
                    isOverviewMode: size.isOverviewMode
                )
                reply(true, nil)
            }
        }
    }

    public func bindTab(hostTabId: String, toInternalId internalId: String, reply: @escaping (Bool) -> Void) {
        NSLog("[ServiceXPC] Received bindTab: host=%@ internal=%@",
              String(hostTabId.prefix(8)), String(internalId.prefix(8)))

        // Dispatch to main thread
        DispatchQueue.main.async { [weak self] in
            guard let self = self else {
                reply(false)
                return
            }

            // Forward to delegate or handle directly
            if let delegate = self.delegate {
                delegate.handleBindTab(hostTabId: hostTabId, internalId: internalId) { success in
                    reply(success)
                }
            } else {
                // Fallback: post notification for backward compatibility
                self.postBindTabNotification(hostTabId: hostTabId, internalId: internalId)
                reply(true)
            }
        }
    }

    public func ping(reply: @escaping (Bool) -> Void) {
        NSLog("[ServiceXPC] Received ping")
        reply(true)
    }

    // MARK: - Notification Fallbacks

    /// Post tab config via internal notification (backward compatibility)
    private func postTabConfigNotification(config: [String: Any]) {
        // Post to self (internal notification for existing handler)
        NotificationCenter.default.post(
            name: NSNotification.Name("ServiceXPC.tabConfig"),
            object: nil,
            userInfo: ["config": config]
        )
    }

    /// Post container size change via DistributedNotification (backward compatibility)
    private func postContainerSizeNotification(tabId: String, size: CGSize, requestId: Int, isOverviewMode: Bool) {
        let sizeInfo: [String: Any] = [
            "tabId": tabId,
            "width": size.width,
            "height": size.height,
            "requestId": "\(requestId)",
            "isOverviewMode": isOverviewMode
        ]

        // Post to self (internal notification for existing handler)
        NotificationCenter.default.post(
            name: NSNotification.Name("ServiceXPC.containerSizeChanged"),
            object: nil,
            userInfo: ["sizeInfo": sizeInfo]
        )
    }

    /// Post bind tab via DistributedNotification (backward compatibility)
    private func postBindTabNotification(hostTabId: String, internalId: String) {
        // Post to self (internal notification for existing handler)
        NotificationCenter.default.post(
            name: NSNotification.Name("ServiceXPC.bindTab"),
            object: nil,
            userInfo: [
                "tabId": hostTabId,
                "internalId": internalId
            ]
        )
    }
}

// MARK: - Delegate Protocol

/// Delegate for handling XPC requests
/// Implemented by ServiceReactNativeHost to process requests
public protocol ServiceRequestHandlerDelegate: AnyObject {
    /// Handle tab configuration
    func handleTabConfig(_ config: [String: Any], completion: @escaping (Bool, String?) -> Void)

    /// Handle container size change
    func handleContainerSizeChanged(
        tabId: String,
        size: CGSize,
        requestId: Int,
        isOverviewMode: Bool,
        completion: @escaping (Bool, String?) -> Void
    )

    /// Handle tab binding
    func handleBindTab(hostTabId: String, internalId: String, completion: @escaping (Bool) -> Void)
}

// MARK: - Optional Default Implementation

public extension ServiceRequestHandlerDelegate {
    func handleTabConfig(_ config: [String: Any], completion: @escaping (Bool, String?) -> Void) {
        completion(false, "Not implemented")
    }

    func handleContainerSizeChanged(
        tabId: String,
        size: CGSize,
        requestId: Int,
        isOverviewMode: Bool,
        completion: @escaping (Bool, String?) -> Void
    ) {
        completion(false, "Not implemented")
    }

    func handleBindTab(hostTabId: String, internalId: String, completion: @escaping (Bool) -> Void) {
        completion(false)
    }
}
