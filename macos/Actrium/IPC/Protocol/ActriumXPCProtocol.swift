//
//  ActriumXPCProtocol.swift
//  Actrium
//
//  XPC protocol definitions for Actrium Host ↔ Service communication.
//
//  Architecture:
//  - ActriumHostXPCProtocol: Host → Service (main app calls XPC service)
//  - ActriumServiceXPCProtocol: Service → Host (XPC service calls back to main app)
//

import Foundation

// MARK: - Host → Service Protocol

/// Protocol for Actrium Host to communicate with Actrium Service
/// This is the primary interface for sending commands to the XPC service.
@objc(ActriumHostXPCProtocol)
public protocol ActriumHostXPCProtocol {
    /// Send tab configuration to the service
    /// - Parameters:
    ///   - config: Tab configuration (NSSecureCoding compliant)
    ///   - reply: Called with success status and optional error message
    @objc func sendTabConfig(
        _ config: IPCXPCTabConfig,
        reply: @escaping (Bool, String?) -> Void
    )

    /// Update container size for a tab
    /// - Parameters:
    ///   - size: Container size information
    ///   - reply: Called with success status and optional error message
    @objc func updateContainerSize(
        _ size: IPCXPCContainerSize,
        reply: @escaping (Bool, String?) -> Void
    )

    /// Bind a host tab ID to an internal service ID
    /// - Parameters:
    ///   - hostTabId: The tab ID from the host app
    ///   - internalId: The internal ID assigned by the service
    ///   - reply: Called with success status
    @objc func bindTab(
        hostTabId: String,
        toInternalId internalId: String,
        reply: @escaping (Bool) -> Void
    )

    /// Ping the service to check connectivity
    /// - Parameter reply: Called with true if service is responsive
    @objc func ping(reply: @escaping (Bool) -> Void)

    /// Request service to prepare for a new tab
    /// - Parameters:
    ///   - tabId: The tab ID to prepare
    ///   - reply: Called when preparation is complete
    @objc func prepareTab(
        tabId: String,
        reply: @escaping (Bool, String?) -> Void
    )

    /// Request service to release resources for a tab
    /// - Parameter tabId: The tab ID to release
    @objc func releaseTab(tabId: String)

    /// Request all tabs to update their overview mode state
    /// - Parameters:
    ///   - isOverviewMode: Whether overview mode is active
    ///   - reply: Called when all tabs have been notified
    @objc func setOverviewMode(
        _ isOverviewMode: Bool,
        reply: @escaping (Bool) -> Void
    )
}

// MARK: - Service → Host Protocol

/// Protocol for Actrium Service to communicate back to Actrium Host
/// This is the callback interface for the XPC service to notify the host.
@objc(ActriumServiceXPCProtocol)
public protocol ActriumServiceXPCProtocol {
    /// Service has become ready
    /// - Parameter bundleId: The service's bundle identifier
    @objc func serviceDidBecomeReady(bundleId: String)

    /// Container size has been applied
    /// - Parameters:
    ///   - tabId: The tab that applied the size
    ///   - width: Applied width
    ///   - height: Applied height
    ///   - requestId: The request ID for correlation
    @objc func sizeDidApply(
        tabId: String,
        width: Double,
        height: Double,
        requestId: Int
    )

    /// Request to open a URL from the service
    /// - Parameters:
    ///   - url: The URL to open
    ///   - tabId: The tab that made the request
    @objc func requestOpenURL(_ url: String, fromTab tabId: String)

    /// Report an error from the service
    /// - Parameter error: Error information
    @objc func reportError(_ error: IPCXPCServiceError)

    /// Tab content has loaded
    /// - Parameters:
    ///   - tabId: The tab that loaded
    ///   - title: The page title (if available)
    @objc func tabDidLoad(tabId: String, title: String?)

    /// Tab is requesting focus
    /// - Parameter tabId: The tab requesting focus
    @objc func tabRequestsFocus(tabId: String)

    /// Console log from the service
    /// - Parameters:
    ///   - level: Log level (debug, info, warn, error)
    ///   - message: Log message
    ///   - tabId: Source tab (if applicable)
    @objc func consoleLog(level: String, message: String, tabId: String?)
}

// MARK: - XPC Service Names

/// Service identifiers for XPC connections
public enum ActriumXPCServiceNames {
    /// Main Actrium View Service (React Native host)
    public static let viewService = "io.actrium.ActriumViewService"

    /// System service variant (bundled apps)
    public static let systemService = "io.actrium.ActriumViewService.system"

    /// ACTR service variant (user apps)
    public static let actrService = "io.actrium.ActriumViewService.actr"

    /// Get service name for a given type
    public static func serviceName(for type: ServiceType) -> String {
        switch type {
        case .system:
            return systemService
        case .actr:
            return actrService
        case .default:
            return viewService
        }
    }

    public enum ServiceType {
        case system
        case actr
        case `default`
    }
}

// MARK: - Protocol Version

/// Protocol version for compatibility checking
public struct ActriumXPCProtocolVersion: Sendable {
    public static let current = ActriumXPCProtocolVersion(major: 1, minor: 0)

    public let major: Int
    public let minor: Int

    public init(major: Int, minor: Int) {
        self.major = major
        self.minor = minor
    }

    /// Check if this version is compatible with another
    public func isCompatible(with other: ActriumXPCProtocolVersion) -> Bool {
        // Major version must match, minor version must be >= other
        major == other.major && minor >= other.minor
    }

    public var description: String {
        "\(major).\(minor)"
    }
}
