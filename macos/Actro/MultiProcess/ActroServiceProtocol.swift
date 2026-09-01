//
//  ActroServiceProtocol.swift
//  ActroService
//
//  XPC Protocol definitions for the Safari Pattern architecture.
//  Actro <-> ActroService <-> ActroTabRenderer
//

import Foundation

// MARK: - Actro -> Service Protocol

/// Protocol for Actro (main app) to communicate with ActroService
@objc(ActroServiceProtocol)
protocol ActroServiceProtocol {
    /// Request to launch a new tab renderer
    /// - Parameters:
    ///   - tabId: Unique identifier for the tab
    ///   - config: Tab configuration (serialized)
    ///   - width: Viewport width
    ///   - height: Viewport height
    ///   - reply: Called with success status and error message
    @objc func launchTab(
        tabId: String,
        config: Data,
        width: Int,
        height: Int,
        reply: @escaping (Bool, String?) -> Void
    )

    /// Terminate a tab renderer
    /// - Parameter tabId: The tab to terminate
    @objc func terminateTab(tabId: String)

    /// Terminate all tab renderers
    @objc func terminateAllTabs()

    /// Register the main app callback for receiving events
    /// - Parameter callback: The callback interface
    @objc func registerMainApp(callback: ActroServiceCallbackProtocol)

    /// Unregister the main app
    @objc func unregisterMainApp()
}

// MARK: - Service -> Actro Callback Protocol

/// Protocol for ActroService to send events back to Actro
@objc(ActroServiceCallbackProtocol)
protocol ActroServiceCallbackProtocol {
    /// Called when a tab renderer is ready with its endpoint for direct connection
    /// - Parameters:
    ///   - tabId: The tab identifier
    ///   - endpoint: The anonymous XPC listener endpoint for direct connection
    @objc func tabDidBecomeReady(tabId: String, endpoint: NSXPCListenerEndpoint)

    /// Called when a tab renderer crashes or terminates unexpectedly
    /// - Parameters:
    ///   - tabId: The tab identifier
    ///   - reason: The termination reason
    @objc func tabDidTerminate(tabId: String, reason: String)

    /// Called when IOSurface is updated
    /// - Parameters:
    ///   - tabId: The tab identifier
    ///   - surfaceID: The IOSurface global ID
    @objc func tabSurfaceDidUpdate(tabId: String, surfaceID: UInt32)
}

// MARK: - Helper -> Service Protocol

/// Protocol for ActroTabRenderer (helper) to communicate with ActroService
@objc(HelperToServiceProtocol)
protocol HelperToServiceProtocol {
    /// Register this helper with the service
    /// - Parameters:
    ///   - tabId: The tab ID this helper is rendering
    ///   - endpoint: The anonymous XPC listener endpoint for Actro to connect directly
    ///   - reply: Called with success status
    @objc func registerHelper(tabId: String, endpoint: NSXPCListenerEndpoint, reply: @escaping (Bool) -> Void)

    /// Report that the helper is ready to render
    /// - Parameters:
    ///   - tabId: The tab ID
    ///   - surfaceID: Initial IOSurface ID
    @objc func helperReady(tabId: String, surfaceID: UInt32)

    /// Report IOSurface update
    /// - Parameters:
    ///   - tabId: The tab ID
    ///   - surfaceID: New IOSurface ID
    @objc func surfaceUpdated(tabId: String, surfaceID: UInt32)

    /// Report helper termination
    /// - Parameters:
    ///   - tabId: The tab ID
    ///   - reason: Termination reason
    @objc func helperTerminating(tabId: String, reason: String)
}

// MARK: - XPC Interface Factory

/// Factory for creating XPC interfaces
enum ActroServiceXPCInterface {
    /// Service name (must match CFBundleIdentifier in Info.plist)
    static let serviceName = "io.actrium.actro.service"

    /// Interface for Actro -> Service communication
    static func serviceInterface() -> NSXPCInterface {
        let interface = NSXPCInterface(with: ActroServiceProtocol.self)

        // Configure callback interface for registerMainApp
        let callbackInterface = callbackInterface()
        interface.setInterface(
            callbackInterface,
            for: #selector(ActroServiceProtocol.registerMainApp(callback:)),
            argumentIndex: 0,
            ofReply: false
        )

        return interface
    }

    /// Interface for Service -> Actro callback
    static func callbackInterface() -> NSXPCInterface {
        let interface = NSXPCInterface(with: ActroServiceCallbackProtocol.self)

        // Allow NSXPCListenerEndpoint in tabDidBecomeReady
        interface.setClasses(
            NSSet(object: NSXPCListenerEndpoint.self) as! Set<AnyHashable>,
            for: #selector(ActroServiceCallbackProtocol.tabDidBecomeReady(tabId:endpoint:)),
            argumentIndex: 1,
            ofReply: false
        )

        return interface
    }

    /// Interface for Helper -> Service communication
    static func helperToServiceInterface() -> NSXPCInterface {
        let interface = NSXPCInterface(with: HelperToServiceProtocol.self)

        // Allow NSXPCListenerEndpoint in registerHelper
        interface.setClasses(
            NSSet(object: NSXPCListenerEndpoint.self) as! Set<AnyHashable>,
            for: #selector(HelperToServiceProtocol.registerHelper(tabId:endpoint:reply:)),
            argumentIndex: 1,
            ofReply: false
        )

        return interface
    }
}
