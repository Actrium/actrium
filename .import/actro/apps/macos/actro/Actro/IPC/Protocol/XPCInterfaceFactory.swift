//
//  XPCInterfaceFactory.swift
//  Actro
//
//  Factory for creating NSXPCInterface instances with proper whitelist configuration.
//  Ensures all custom types are properly registered for NSSecureCoding.
//

import Foundation

// MARK: - XPC Interface Factory

/// Factory for creating properly configured XPC interfaces
public enum XPCInterfaceFactory {
    // MARK: - Host Interface

    /// Create the interface for Host → Service communication
    public static func hostInterface() -> NSXPCInterface {
        let interface = NSXPCInterface(with: ActroHostXPCProtocol.self)

        // Configure allowed classes for sendTabConfig
        let tabConfigClasses = NSSet(array: [IPCXPCTabConfig.self]) as! Set<AnyHashable>
        interface.setClasses(
            tabConfigClasses,
            for: #selector(ActroHostXPCProtocol.sendTabConfig(_:reply:)),
            argumentIndex: 0,
            ofReply: false
        )

        // Configure allowed classes for updateContainerSize
        let containerSizeClasses = NSSet(array: [IPCXPCContainerSize.self]) as! Set<AnyHashable>
        interface.setClasses(
            containerSizeClasses,
            for: #selector(ActroHostXPCProtocol.updateContainerSize(_:reply:)),
            argumentIndex: 0,
            ofReply: false
        )

        return interface
    }

    // MARK: - Service Interface

    /// Create the interface for Service → Host communication (callbacks)
    public static func serviceInterface() -> NSXPCInterface {
        let interface = NSXPCInterface(with: ActroServiceXPCProtocol.self)

        // Configure allowed classes for reportError
        let errorClasses = NSSet(array: [IPCXPCServiceError.self]) as! Set<AnyHashable>
        interface.setClasses(
            errorClasses,
            for: #selector(ActroServiceXPCProtocol.reportError(_:)),
            argumentIndex: 0,
            ofReply: false
        )

        return interface
    }

    // MARK: - All Allowed Classes

    /// Get all classes that are allowed in XPC communication
    public static var allAllowedClasses: [AnyClass] {
        [
            IPCXPCTabConfig.self,
            IPCXPCContainerSize.self,
            IPCXPCServiceError.self,
            IPCXPCServiceReady.self,
            NSString.self,
            NSNumber.self,
            NSDate.self,
            NSData.self,
            NSArray.self,
            NSDictionary.self,
            NSURL.self
        ]
    }

    /// Get NSSet of all allowed classes for generic XPC operations
    public static var allAllowedClassesSet: Set<AnyHashable> {
        NSSet(array: allAllowedClasses) as! Set<AnyHashable>
    }
}

// MARK: - Connection Factory

/// Factory for creating XPC connections
public enum XPCConnectionFactory {
    /// Create an XPC connection to a Actro service
    /// - Parameters:
    ///   - serviceId: The service bundle identifier
    ///   - hostCallback: Object implementing ActroServiceXPCProtocol for callbacks
    /// - Returns: Configured but not yet resumed NSXPCConnection
    public static func createConnection(
        to serviceId: String,
        hostCallback: ActroServiceXPCProtocol? = nil
    ) -> NSXPCConnection {
        let connection = NSXPCConnection(serviceName: serviceId)

        // Configure remote object interface (what we call)
        connection.remoteObjectInterface = XPCInterfaceFactory.hostInterface()

        // Configure exported interface (what service calls on us)
        if let callback = hostCallback {
            connection.exportedInterface = XPCInterfaceFactory.serviceInterface()
            connection.exportedObject = callback
        }

        return connection
    }

    /// Create an XPC connection with interruption and invalidation handlers
    public static func createManagedConnection(
        to serviceId: String,
        hostCallback: ActroServiceXPCProtocol? = nil,
        onInterruption: @escaping () -> Void,
        onInvalidation: @escaping () -> Void
    ) -> NSXPCConnection {
        let connection = createConnection(to: serviceId, hostCallback: hostCallback)

        connection.interruptionHandler = onInterruption
        connection.invalidationHandler = onInvalidation

        return connection
    }
}

// MARK: - Code Signature Validation

/// Utilities for validating XPC connection code signatures
public enum XPCCodeSignatureValidator {
    /// Validate that a connection comes from a trusted process
    /// - Parameter connection: The XPC connection to validate
    /// - Returns: True if the connection is from a trusted process
    public static func validateConnection(_ connection: NSXPCConnection) -> Bool {
        // Get the audit token for the remote process
        let auditToken = connection.auditToken

        // In production, you would:
        // 1. Use SecCodeCopyGuestWithAttributes to get the code object
        // 2. Use SecCodeCheckValidity to verify the signature
        // 3. Check the code signing requirements match your app

        // For now, we trust connections from our own process
        // This should be enhanced with proper code signing verification
        guard auditToken != nil else {
            return false
        }

        return true
    }

    /// Get the bundle identifier of the remote process
    /// - Parameter connection: The XPC connection
    /// - Returns: Bundle identifier if available
    public static func remoteBundleIdentifier(_ connection: NSXPCConnection) -> String? {
        // This would require using Security framework to extract
        // the bundle identifier from the code signature
        return nil
    }
}

// MARK: - NSXPCConnection Extension

extension NSXPCConnection {
    /// Audit token for the remote process
    var auditToken: audit_token_t? {
        // audit_token_t is available on NSXPCConnection
        // Access via private API or use processIdentifier instead
        return nil
    }

    /// Get the remote process identifier
    var remoteProcessIdentifier: pid_t {
        return processIdentifier
    }
}
