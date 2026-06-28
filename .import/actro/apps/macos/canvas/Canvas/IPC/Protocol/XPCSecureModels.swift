//
//  XPCSecureModels.swift
//  Canvas
//
//  NSSecureCoding compliant models for XPC communication.
//  These models can be safely transferred across process boundaries.
//

import Foundation
import AppKit

// MARK: - Tab Configuration

/// Tab configuration for XPC transfer
/// Contains all information needed to initialize or update a tab in the service.
@objc(IPCXPCTabConfig)
public final class IPCXPCTabConfig: NSObject, NSSecureCoding {
    public static var supportsSecureCoding: Bool { true }

    /// Unique tab identifier
    @objc public let tabId: String

    /// Tab title
    @objc public let title: String

    /// Container width
    @objc public let containerWidth: CGFloat

    /// Container height
    @objc public let containerHeight: CGFloat

    /// Whether overview mode is active
    @objc public let isOverviewMode: Bool

    /// Background color (hex string)
    @objc public let backgroundColor: String

    /// System app type (if system app)
    @objc public let systemAppType: String?

    /// Unified script content
    @objc public let unifiedScript: String?

    /// Left panel script content
    @objc public let leftPanelScript: String?

    /// Whether left panel is visible
    @objc public let leftPanelVisible: Bool

    /// Whether right panel is visible
    @objc public let rightPanelVisible: Bool

    /// Additional configuration as JSON string
    @objc public let configJSON: String?

    /// Request ID for correlation
    @objc public let requestId: Int

    public init(
        tabId: String,
        title: String,
        containerWidth: CGFloat,
        containerHeight: CGFloat,
        isOverviewMode: Bool,
        backgroundColor: String = "#1f1f1f",
        systemAppType: String? = nil,
        unifiedScript: String? = nil,
        leftPanelScript: String? = nil,
        leftPanelVisible: Bool = true,
        rightPanelVisible: Bool = false,
        configJSON: String? = nil,
        requestId: Int = 0
    ) {
        self.tabId = tabId
        self.title = title
        self.containerWidth = containerWidth
        self.containerHeight = containerHeight
        self.isOverviewMode = isOverviewMode
        self.backgroundColor = backgroundColor
        self.systemAppType = systemAppType
        self.unifiedScript = unifiedScript
        self.leftPanelScript = leftPanelScript
        self.leftPanelVisible = leftPanelVisible
        self.rightPanelVisible = rightPanelVisible
        self.configJSON = configJSON
        self.requestId = requestId
        super.init()
    }

    // MARK: - NSSecureCoding

    public required init?(coder: NSCoder) {
        guard let tabId = coder.decodeObject(of: NSString.self, forKey: "tabId") as String?,
              let title = coder.decodeObject(of: NSString.self, forKey: "title") as String?,
              let backgroundColor = coder.decodeObject(of: NSString.self, forKey: "backgroundColor") as String? else {
            return nil
        }

        self.tabId = tabId
        self.title = title
        self.containerWidth = CGFloat(coder.decodeDouble(forKey: "containerWidth"))
        self.containerHeight = CGFloat(coder.decodeDouble(forKey: "containerHeight"))
        self.isOverviewMode = coder.decodeBool(forKey: "isOverviewMode")
        self.backgroundColor = backgroundColor
        self.systemAppType = coder.decodeObject(of: NSString.self, forKey: "systemAppType") as String?
        self.unifiedScript = coder.decodeObject(of: NSString.self, forKey: "unifiedScript") as String?
        self.leftPanelScript = coder.decodeObject(of: NSString.self, forKey: "leftPanelScript") as String?
        self.leftPanelVisible = coder.decodeBool(forKey: "leftPanelVisible")
        self.rightPanelVisible = coder.decodeBool(forKey: "rightPanelVisible")
        self.configJSON = coder.decodeObject(of: NSString.self, forKey: "configJSON") as String?
        self.requestId = coder.decodeInteger(forKey: "requestId")
        super.init()
    }

    public func encode(with coder: NSCoder) {
        coder.encode(tabId as NSString, forKey: "tabId")
        coder.encode(title as NSString, forKey: "title")
        coder.encode(Double(containerWidth), forKey: "containerWidth")
        coder.encode(Double(containerHeight), forKey: "containerHeight")
        coder.encode(isOverviewMode, forKey: "isOverviewMode")
        coder.encode(backgroundColor as NSString, forKey: "backgroundColor")
        coder.encode(systemAppType as NSString?, forKey: "systemAppType")
        coder.encode(unifiedScript as NSString?, forKey: "unifiedScript")
        coder.encode(leftPanelScript as NSString?, forKey: "leftPanelScript")
        coder.encode(leftPanelVisible, forKey: "leftPanelVisible")
        coder.encode(rightPanelVisible, forKey: "rightPanelVisible")
        coder.encode(configJSON as NSString?, forKey: "configJSON")
        coder.encode(requestId, forKey: "requestId")
    }

    // MARK: - Convenience

    /// Container size as CGSize
    public var containerSize: CGSize {
        CGSize(width: containerWidth, height: containerHeight)
    }
}

// MARK: - Container Size

/// Container size information for XPC transfer
@objc(IPCXPCContainerSize)
public final class IPCXPCContainerSize: NSObject, NSSecureCoding {
    public static var supportsSecureCoding: Bool { true }

    /// Tab identifier
    @objc public let tabId: String

    /// Container width
    @objc public let width: CGFloat

    /// Container height
    @objc public let height: CGFloat

    /// Request ID for correlation
    @objc public let requestId: Int

    /// Whether overview mode is active
    @objc public let isOverviewMode: Bool

    public init(
        tabId: String,
        width: CGFloat,
        height: CGFloat,
        requestId: Int = 0,
        isOverviewMode: Bool = false
    ) {
        self.tabId = tabId
        self.width = width
        self.height = height
        self.requestId = requestId
        self.isOverviewMode = isOverviewMode
        super.init()
    }

    /// Initialize from CGSize
    public convenience init(tabId: String, size: CGSize, requestId: Int = 0, isOverviewMode: Bool = false) {
        self.init(tabId: tabId, width: size.width, height: size.height, requestId: requestId, isOverviewMode: isOverviewMode)
    }

    // MARK: - NSSecureCoding

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

    // MARK: - Convenience

    /// Size as CGSize
    public var size: CGSize {
        CGSize(width: width, height: height)
    }
}

// MARK: - Service Error

/// Service error information for XPC transfer
@objc(IPCXPCServiceError)
public final class IPCXPCServiceError: NSObject, NSSecureCoding {
    public static var supportsSecureCoding: Bool { true }

    /// Error code
    @objc public let code: Int

    /// Error message
    @objc public let message: String

    /// Source tab ID (if applicable)
    @objc public let tabId: String?

    /// Whether the error is recoverable
    @objc public let isRecoverable: Bool

    /// Additional context as JSON string
    @objc public let contextJSON: String?

    /// Timestamp when error occurred
    @objc public let timestamp: Date

    public init(
        code: Int,
        message: String,
        tabId: String? = nil,
        isRecoverable: Bool = false,
        contextJSON: String? = nil
    ) {
        self.code = code
        self.message = message
        self.tabId = tabId
        self.isRecoverable = isRecoverable
        self.contextJSON = contextJSON
        self.timestamp = Date()
        super.init()
    }

    // MARK: - NSSecureCoding

    public required init?(coder: NSCoder) {
        self.code = coder.decodeInteger(forKey: "code")
        guard let message = coder.decodeObject(of: NSString.self, forKey: "message") as String?,
              let timestamp = coder.decodeObject(of: NSDate.self, forKey: "timestamp") as Date? else {
            return nil
        }

        self.message = message
        self.tabId = coder.decodeObject(of: NSString.self, forKey: "tabId") as String?
        self.isRecoverable = coder.decodeBool(forKey: "isRecoverable")
        self.contextJSON = coder.decodeObject(of: NSString.self, forKey: "contextJSON") as String?
        self.timestamp = timestamp
        super.init()
    }

    public func encode(with coder: NSCoder) {
        coder.encode(code, forKey: "code")
        coder.encode(message as NSString, forKey: "message")
        coder.encode(tabId as NSString?, forKey: "tabId")
        coder.encode(isRecoverable, forKey: "isRecoverable")
        coder.encode(contextJSON as NSString?, forKey: "contextJSON")
        coder.encode(timestamp as NSDate, forKey: "timestamp")
    }

    // MARK: - Convenience

    /// Convert to IPCError
    public func toIPCError() -> IPCError {
        IPCError(
            code: IPCErrorCode(rawValue: code) ?? .unknownError,
            message: message,
            tabId: tabId.flatMap { UUID(uuidString: $0) }
        )
    }
}

// MARK: - Service Ready

/// Service ready notification for XPC transfer
@objc(IPCXPCServiceReady)
public final class IPCXPCServiceReady: NSObject, NSSecureCoding {
    public static var supportsSecureCoding: Bool { true }

    /// Service bundle identifier
    @objc public let bundleId: String

    /// Host process ID
    @objc public let hostProcessId: Int32

    /// Protocol version
    @objc public let protocolVersion: String

    public init(bundleId: String, hostProcessId: Int32, protocolVersion: String = "1.0") {
        self.bundleId = bundleId
        self.hostProcessId = hostProcessId
        self.protocolVersion = protocolVersion
        super.init()
    }

    // MARK: - NSSecureCoding

    public required init?(coder: NSCoder) {
        guard let bundleId = coder.decodeObject(of: NSString.self, forKey: "bundleId") as String?,
              let protocolVersion = coder.decodeObject(of: NSString.self, forKey: "protocolVersion") as String? else {
            return nil
        }

        self.bundleId = bundleId
        self.hostProcessId = coder.decodeInt32(forKey: "hostProcessId")
        self.protocolVersion = protocolVersion
        super.init()
    }

    public func encode(with coder: NSCoder) {
        coder.encode(bundleId as NSString, forKey: "bundleId")
        coder.encode(hostProcessId, forKey: "hostProcessId")
        coder.encode(protocolVersion as NSString, forKey: "protocolVersion")
    }
}
