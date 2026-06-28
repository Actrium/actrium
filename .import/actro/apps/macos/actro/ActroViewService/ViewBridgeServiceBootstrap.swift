//
//  ViewBridgeServiceBootstrap.swift
//  Actro
//
//  Shared utilities for ViewBridge XPC service entry points.
//  Used by both ActroViewService and ActroTerminalService.
//

import Foundation
import Cocoa
import ObjectiveC

// MARK: - Service ViewController Registration

/// Register an NSServiceViewController subclass via ObjC runtime.
///
/// ViewBridge requires an NSServiceViewController subclass to be registered before
/// NSViewServiceApplicationMain is called. Since NSServiceViewController is private API,
/// we create the subclass dynamically using objc_allocateClassPair.
///
/// - Parameters:
///   - className: The class name to register (e.g. "TerminalServiceViewController")
///   - loadView: Block called when the VC's loadView is invoked
///   - viewDidLayout: Optional block called when the VC's viewDidLayout is invoked
///   - dealloc: Optional block called when the VC is deallocated
///   - additionalSetup: Optional closure for adding extra methods/ivars to the class
/// - Returns: true if registration succeeded
@discardableResult
func registerServiceViewControllerClass(
    className: String,
    loadView: @escaping @convention(block) (AnyObject) -> Void,
    viewDidLayout: (@convention(block) (AnyObject) -> Void)? = nil,
    dealloc: (@convention(block) (AnyObject) -> Void)? = nil,
    additionalSetup: ((AnyClass) -> Void)? = nil
) -> Bool {
    let frameworkPath = "/System/Library/PrivateFrameworks/ViewBridge.framework/ViewBridge"
    guard dlopen(frameworkPath, RTLD_NOW) != nil else {
        NSLog("[ViewBridgeBootstrap] Failed to load ViewBridge")
        return false
    }

    guard let superclass = NSClassFromString("NSServiceViewController") else {
        NSLog("[ViewBridgeBootstrap] NSServiceViewController not found")
        return false
    }

    if NSClassFromString(className) != nil {
        NSLog("[ViewBridgeBootstrap] %@ already registered", className)
        return true
    }

    guard let newClass = objc_allocateClassPair(superclass, className, 0) else {
        NSLog("[ViewBridgeBootstrap] Failed to allocate class pair for %@", className)
        return false
    }

    // loadView
    let loadViewIMP = imp_implementationWithBlock(loadView)
    class_addMethod(newClass, NSSelectorFromString("loadView"), loadViewIMP, "v@:")

    // viewDidLayout
    if let viewDidLayout = viewDidLayout {
        let imp = imp_implementationWithBlock(viewDidLayout)
        class_addMethod(newClass, NSSelectorFromString("viewDidLayout"), imp, "v@:")
    }

    // dealloc
    if let dealloc = dealloc {
        let imp = imp_implementationWithBlock(dealloc)
        class_addMethod(newClass, NSSelectorFromString("dealloc"), imp, "v@:")
    }

    // Additional methods/ivars
    additionalSetup?(newClass)

    objc_registerClassPair(newClass)
    NSLog("[ViewBridgeBootstrap] %@ registered successfully", className)
    return true
}

// MARK: - Service Entry Point

/// ViewBridge service main entry point.
///
/// Loads the ViewBridge framework and calls NSViewServiceApplicationMain.
/// This function does not return (calls exit()).
///
/// - Parameter serviceName: Name for logging (e.g. "ActroTerminalService")
func viewBridgeServiceMain(serviceName: String) -> Never {
    let frameworkPath = "/System/Library/PrivateFrameworks/ViewBridge.framework/ViewBridge"

    guard let handle = dlopen(frameworkPath, RTLD_NOW) else {
        let error = dlerror().map { String(cString: $0) } ?? "Unknown error"
        NSLog("%@: Failed to load ViewBridge framework: %@", serviceName, error)
        exit(1)
    }

    guard let symbol = dlsym(handle, "NSViewServiceApplicationMain") else {
        NSLog("%@: NSViewServiceApplicationMain symbol not found", serviceName)
        exit(1)
    }

    typealias MainFunc = @convention(c) () -> Int32
    let viewServiceMain = unsafeBitCast(symbol, to: MainFunc.self)

    NSLog("%@: Starting view service...", serviceName)
    exit(viewServiceMain())
}
