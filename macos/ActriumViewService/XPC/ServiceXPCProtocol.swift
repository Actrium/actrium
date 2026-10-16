//
//  ServiceXPCProtocol.swift
//  ActriumViewService
//
//  XPC protocol definitions for Service side (mirrors Host-side ActriumXPCProtocol.swift).
//  Both sides must have matching protocol definitions for NSXPCConnection to work.
//

import Foundation
import AppKit

// MARK: - Host → Service Protocol

/// Protocol for Host-to-Service XPC calls
/// Must match ActriumHostXPCProtocol in Actrium/IPC/Protocol/ActriumXPCProtocol.swift
@objc(ActriumHostXPCProtocol)
public protocol ActriumHostXPCProtocol {
    /// Send tab configuration to Service
    /// - Parameters:
    ///   - config: Tab configuration (NSSecureCoding)
    ///   - reply: Completion with success flag and optional error message
    func sendTabConfig(_ config: ServiceXPCTabConfig, reply: @escaping (Bool, String?) -> Void)

    /// Update container size for a tab
    /// - Parameters:
    ///   - size: Container size info (NSSecureCoding)
    ///   - reply: Completion with success flag and optional error message
    func updateContainerSize(_ size: ServiceXPCContainerSize, reply: @escaping (Bool, String?) -> Void)

    /// Bind a host tab ID to an internal connection ID
    /// - Parameters:
    ///   - hostTabId: The tab ID from Host (UUID string)
    ///   - internalId: The internal connection ID from Service
    ///   - reply: Completion with success flag
    func bindTab(hostTabId: String, toInternalId internalId: String, reply: @escaping (Bool) -> Void)

    /// Ping to check Service availability
    /// - Parameter reply: Completion with reachability status
    func ping(reply: @escaping (Bool) -> Void)
}

// MARK: - Service → Host Protocol

/// Protocol for Service-to-Host XPC callbacks
/// Must match ActriumServiceXPCProtocol in Actrium/IPC/Protocol/ActriumXPCProtocol.swift exactly
@objc(ActriumServiceXPCProtocol)
public protocol ActriumServiceXPCProtocol {
    /// Service has become ready
    /// - Parameter bundleId: The service's bundle identifier
    func serviceDidBecomeReady(bundleId: String)

    /// Container size has been applied
    /// - Parameters:
    ///   - tabId: The tab that applied the size
    ///   - width: Applied width
    ///   - height: Applied height
    ///   - requestId: The request ID for correlation
    func sizeDidApply(tabId: String, width: Double, height: Double, requestId: Int)

    /// Request to open a URL from the service
    /// - Parameters:
    ///   - url: The URL to open
    ///   - tabId: The tab that made the request
    func requestOpenURL(_ url: String, fromTab tabId: String)

    /// Report an error from the service
    /// - Parameter error: Error information
    func reportError(_ error: ServiceXPCServiceError)

    /// Tab content has loaded
    /// - Parameters:
    ///   - tabId: The tab that loaded
    ///   - title: The page title (if available)
    func tabDidLoad(tabId: String, title: String?)

    /// Tab is requesting focus
    /// - Parameter tabId: The tab requesting focus
    func tabRequestsFocus(tabId: String)

    /// Console log from the service
    /// - Parameters:
    ///   - level: Log level (debug, info, warn, error)
    ///   - message: Log message
    ///   - tabId: Source tab (if applicable)
    func consoleLog(level: String, message: String, tabId: String?)
}

// MARK: - NSSecureCoding Models

/// Tab configuration for XPC transfer
/// Must match IPCXPCTabConfig in Actrium/IPC/Protocol/XPCSecureModels.swift
/// @objc name must match exactly for NSSecureCoding across XPC
@objc(IPCXPCTabConfig)
public final class ServiceXPCTabConfig: NSObject, NSSecureCoding {
    public static var supportsSecureCoding: Bool { true }

    @objc public let tabId: String
    @objc public let title: String
    @objc public let containerWidth: CGFloat
    @objc public let containerHeight: CGFloat
    @objc public let isOverviewMode: Bool
    @objc public let configJSON: String

    public init(
        tabId: String,
        title: String,
        containerWidth: CGFloat,
        containerHeight: CGFloat,
        isOverviewMode: Bool,
        configJSON: String
    ) {
        self.tabId = tabId
        self.title = title
        self.containerWidth = containerWidth
        self.containerHeight = containerHeight
        self.isOverviewMode = isOverviewMode
        self.configJSON = configJSON
        super.init()
    }

    public required init?(coder: NSCoder) {
        guard let tabId = coder.decodeObject(of: NSString.self, forKey: "tabId") as String?,
              let title = coder.decodeObject(of: NSString.self, forKey: "title") as String?,
              let configJSON = coder.decodeObject(of: NSString.self, forKey: "configJSON") as String? else {
            return nil
        }
        self.tabId = tabId
        self.title = title
        self.containerWidth = CGFloat(coder.decodeDouble(forKey: "containerWidth"))
        self.containerHeight = CGFloat(coder.decodeDouble(forKey: "containerHeight"))
        self.isOverviewMode = coder.decodeBool(forKey: "isOverviewMode")
        self.configJSON = configJSON
        super.init()
    }

    public func encode(with coder: NSCoder) {
        coder.encode(tabId as NSString, forKey: "tabId")
        coder.encode(title as NSString, forKey: "title")
        coder.encode(Double(containerWidth), forKey: "containerWidth")
        coder.encode(Double(containerHeight), forKey: "containerHeight")
        coder.encode(isOverviewMode, forKey: "isOverviewMode")
        coder.encode(configJSON as NSString, forKey: "configJSON")
    }
}

/// Container size for XPC transfer
/// Must match IPCXPCContainerSize in Actrium/IPC/Protocol/XPCSecureModels.swift
/// @objc name must match exactly for NSSecureCoding across XPC
@objc(IPCXPCContainerSize)
public final class ServiceXPCContainerSize: NSObject, NSSecureCoding {
    public static var supportsSecureCoding: Bool { true }

    @objc public let tabId: String
    @objc public let width: CGFloat
    @objc public let height: CGFloat
    @objc public let requestId: Int
    @objc public let isOverviewMode: Bool

    public init(tabId: String, width: CGFloat, height: CGFloat, requestId: Int, isOverviewMode: Bool) {
        self.tabId = tabId
        self.width = width
        self.height = height
        self.requestId = requestId
        self.isOverviewMode = isOverviewMode
        super.init()
    }

    public required init?(coder: NSCoder) {
        guard let tabId = coder.decodeObject(of: NSString.self, forKey: "tabId") as String? else {
            return nil
        }
        self.tabId = tabId
        self.width = CGFloat(coder.decodeDouble(forKey: "width"))
        self.height = CGFloat(coder.decodeDouble(forKey: "height"))
        self.requestId = coder.decodeInteger(forKey: "requestId")
        self.isOverviewMode = coder.decodeBool(forKey: "isOverviewMode")
        super.init()
    }

    public func encode(with coder: NSCoder) {
        coder.encode(tabId as NSString, forKey: "tabId")
        coder.encode(Double(width), forKey: "width")
        coder.encode(Double(height), forKey: "height")
        coder.encode(requestId, forKey: "requestId")
        coder.encode(isOverviewMode, forKey: "isOverviewMode")
    }

    public var size: CGSize {
        CGSize(width: width, height: height)
    }
}

/// Service ready notification
/// Must match IPCXPCServiceReady in Actrium/IPC/Protocol/XPCSecureModels.swift
/// @objc name must match exactly for NSSecureCoding across XPC
@objc(IPCXPCServiceReady)
public final class ServiceXPCServiceReady: NSObject, NSSecureCoding {
    public static var supportsSecureCoding: Bool { true }

    @objc public let bundleId: String
    @objc public let processId: Int32
    @objc public let timestamp: Date

    public init(bundleId: String, processId: Int32, timestamp: Date = Date()) {
        self.bundleId = bundleId
        self.processId = processId
        self.timestamp = timestamp
        super.init()
    }

    public required init?(coder: NSCoder) {
        guard let bundleId = coder.decodeObject(of: NSString.self, forKey: "bundleId") as String?,
              let timestamp = coder.decodeObject(of: NSDate.self, forKey: "timestamp") as Date? else {
            return nil
        }
        self.bundleId = bundleId
        self.processId = coder.decodeInt32(forKey: "processId")
        self.timestamp = timestamp
        super.init()
    }

    public func encode(with coder: NSCoder) {
        coder.encode(bundleId as NSString, forKey: "bundleId")
        coder.encode(processId, forKey: "processId")
        coder.encode(timestamp as NSDate, forKey: "timestamp")
    }
}

/// Service error for XPC transfer
/// Must match IPCXPCServiceError in Actrium/IPC/Protocol/XPCSecureModels.swift
/// @objc name must match exactly for NSSecureCoding across XPC
@objc(IPCXPCServiceError)
public final class ServiceXPCServiceError: NSObject, NSSecureCoding {
    public static var supportsSecureCoding: Bool { true }

    @objc public let code: Int
    @objc public let message: String
    @objc public let tabId: String?
    @objc public let timestamp: Date

    public init(code: Int, message: String, tabId: String? = nil, timestamp: Date = Date()) {
        self.code = code
        self.message = message
        self.tabId = tabId
        self.timestamp = timestamp
        super.init()
    }

    public required init?(coder: NSCoder) {
        guard let message = coder.decodeObject(of: NSString.self, forKey: "message") as String?,
              let timestamp = coder.decodeObject(of: NSDate.self, forKey: "timestamp") as Date? else {
            return nil
        }
        self.code = coder.decodeInteger(forKey: "code")
        self.message = message
        self.tabId = coder.decodeObject(of: NSString.self, forKey: "tabId") as String?
        self.timestamp = timestamp
        super.init()
    }

    public func encode(with coder: NSCoder) {
        coder.encode(code, forKey: "code")
        coder.encode(message as NSString, forKey: "message")
        if let tabId = tabId {
            coder.encode(tabId as NSString, forKey: "tabId")
        }
        coder.encode(timestamp as NSDate, forKey: "timestamp")
    }
}

// MARK: - XPC Interface Factory

/// Factory for creating XPC interfaces with proper whitelist configuration
public enum ServiceXPCInterfaceFactory {
    /// Create interface for Host → Service protocol
    public static func createHostToServiceInterface() -> NSXPCInterface {
        let interface = NSXPCInterface(with: ActriumHostXPCProtocol.self)

        // Whitelist allowed classes for sendTabConfig:
        let tabConfigClasses = NSSet(array: [
            ServiceXPCTabConfig.self,
            NSString.self,
            NSNumber.self,
            NSDate.self
        ])
        interface.setClasses(
            tabConfigClasses as! Set<AnyHashable>,
            for: #selector(ActriumHostXPCProtocol.sendTabConfig(_:reply:)),
            argumentIndex: 0,
            ofReply: false
        )

        // Whitelist allowed classes for updateContainerSize:
        let containerSizeClasses = NSSet(array: [
            ServiceXPCContainerSize.self,
            NSString.self,
            NSNumber.self
        ])
        interface.setClasses(
            containerSizeClasses as! Set<AnyHashable>,
            for: #selector(ActriumHostXPCProtocol.updateContainerSize(_:reply:)),
            argumentIndex: 0,
            ofReply: false
        )

        return interface
    }

    /// Create interface for Service → Host protocol
    public static func createServiceToHostInterface() -> NSXPCInterface {
        let interface = NSXPCInterface(with: ActriumServiceXPCProtocol.self)

        // Whitelist allowed classes for reportError:
        let errorClasses = NSSet(array: [
            ServiceXPCServiceError.self,
            NSString.self,
            NSNumber.self,
            NSDate.self
        ])
        interface.setClasses(
            errorClasses as! Set<AnyHashable>,
            for: #selector(ActriumServiceXPCProtocol.reportError(_:)),
            argumentIndex: 0,
            ofReply: false
        )

        return interface
    }
}
