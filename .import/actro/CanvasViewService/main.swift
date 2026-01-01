//
//  main.swift
//  CanvasViewService
//
//  ViewBridge XPC service entry point for cross-process React Native rendering.
//

import Foundation

// Load ViewBridge framework and start the view service application
let frameworkPath = "/System/Library/PrivateFrameworks/ViewBridge.framework/ViewBridge"

guard let handle = dlopen(frameworkPath, RTLD_NOW) else {
    let error = String(cString: dlerror())
    NSLog("CanvasViewService: Failed to load ViewBridge: \(error)")
    exit(1)
}

guard let symbol = dlsym(handle, "NSViewServiceApplicationMain") else {
    NSLog("CanvasViewService: NSViewServiceApplicationMain not found")
    exit(1)
}

typealias MainFunc = @convention(c) () -> Int32
let mainFunc = unsafeBitCast(symbol, to: MainFunc.self)

NSLog("CanvasViewService: Starting view service")
exit(mainFunc())
