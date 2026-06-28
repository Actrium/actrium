//
//  ViewBridgeHostUtils.swift
//  Actro
//
//  Shared utility for requesting remote NSViewControllers from ViewBridge XPC services.
//  Used by both TerminalXPCBridge and XPCServicePool on the host side.
//

import Foundation
import AppKit

// MARK: - Errors

enum ViewBridgeHostError: Error, LocalizedError {
    case viewBridgeUnavailable
    case remoteViewControllerClassNotFound
    case requestMethodNotFound
    case connectionFailed

    var errorDescription: String? {
        switch self {
        case .viewBridgeUnavailable: return "ViewBridge framework not available"
        case .remoteViewControllerClassNotFound: return "NSRemoteViewController class not found"
        case .requestMethodNotFound: return "requestViewController method not found"
        case .connectionFailed: return "XPC connection failed"
        }
    }
}

// MARK: - Remote ViewController Request

/// Request a remote NSViewController from a ViewBridge XPC service.
///
/// Loads the ViewBridge framework, resolves NSRemoteViewController, and calls
/// `requestViewController:fromServiceWithBundleIdentifier:connectionHandler:`.
///
/// - Parameters:
///   - className: The VC class name in the service (e.g. "TerminalServiceViewController")
///   - bundleIdentifier: The XPC service bundle identifier
///   - completion: Called on main thread with the remote VC or error
func requestRemoteViewController(
    className: String,
    bundleIdentifier: String,
    completion: @escaping (NSViewController?, Error?) -> Void
) {
    let frameworkPath = "/System/Library/PrivateFrameworks/ViewBridge.framework/ViewBridge"
    guard dlopen(frameworkPath, RTLD_NOW) != nil else {
        completion(nil, ViewBridgeHostError.viewBridgeUnavailable)
        return
    }

    guard let remoteVCClass = NSClassFromString("NSRemoteViewController") else {
        completion(nil, ViewBridgeHostError.remoteViewControllerClassNotFound)
        return
    }

    let selector = NSSelectorFromString("requestViewController:fromServiceWithBundleIdentifier:connectionHandler:")
    guard remoteVCClass.responds(to: selector) else {
        completion(nil, ViewBridgeHostError.requestMethodNotFound)
        return
    }

    let handler: @convention(block) (NSViewController?, Error?) -> Void = { viewController, error in
        DispatchQueue.main.async {
            if let vc = viewController {
                completion(vc, nil)
            } else {
                completion(nil, error ?? ViewBridgeHostError.connectionFailed)
            }
        }
    }

    let method = remoteVCClass.method(for: selector)
    typealias RequestFunc = @convention(c) (AnyClass, Selector, String, String, Any) -> Void
    let requestFunc = unsafeBitCast(method, to: RequestFunc.self)

    requestFunc(remoteVCClass, selector, className, bundleIdentifier, handler)
}
