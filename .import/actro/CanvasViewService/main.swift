//
//  main.swift
//  CanvasViewService
//
//  ViewBridge XPC service entry point.
//  Uses NSViewServiceApplicationMain instead of standard XPC listener.
//

import Foundation
import Cocoa

// Register our view controller class before starting the service
registerCanvasServiceViewController()

// Load ViewBridge framework
let frameworkPath = "/System/Library/PrivateFrameworks/ViewBridge.framework/ViewBridge"

guard let handle = dlopen(frameworkPath, RTLD_NOW) else {
    let error = dlerror().map { String(cString: $0) } ?? "Unknown error"
    NSLog("CanvasViewService: Failed to load ViewBridge framework: \(error)")
    exit(1)
}

NSLog("CanvasViewService: ViewBridge framework loaded")

// Get NSViewServiceApplicationMain function
guard let symbol = dlsym(handle, "NSViewServiceApplicationMain") else {
    NSLog("CanvasViewService: NSViewServiceApplicationMain symbol not found")
    exit(1)
}

typealias MainFunc = @convention(c) () -> Int32
let viewServiceMain = unsafeBitCast(symbol, to: MainFunc.self)

NSLog("CanvasViewService: Starting view service...")
exit(viewServiceMain())
