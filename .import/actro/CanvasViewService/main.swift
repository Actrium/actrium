//
//  main.swift
//  CanvasViewService
//
//  ViewBridge XPC service entry point.
//  Uses NSViewServiceApplicationMain instead of standard XPC listener.
//

import Foundation
import Cocoa

// Early startup logging - write to stderr which always works
fputs("[XPC Service Starting]\n", stderr)
fputs("Time: \(Date())\n", stderr)
fputs("PID: \(ProcessInfo.processInfo.processIdentifier)\n", stderr)
fputs("Bundle: \(Bundle.main.bundlePath)\n", stderr)
fflush(stderr)

// Also try to write to temp file
let tempDir = NSTemporaryDirectory()
let logPath = (tempDir as NSString).appendingPathComponent("canvas-xpc-service.log")
fputs("TempDir: \(tempDir)\n", stderr)
fputs("LogPath: \(logPath)\n", stderr)
fflush(stderr)

let startupLog = """
[XPC Service Starting]
Time: \(Date())
PID: \(ProcessInfo.processInfo.processIdentifier)
Bundle: \(Bundle.main.bundlePath)
TempDir: \(tempDir)

"""
do {
    try startupLog.write(toFile: logPath, atomically: true, encoding: .utf8)
    fputs("Log file written successfully to: \(logPath)\n", stderr)
} catch {
    fputs("Failed to write log file: \(error)\n", stderr)
}
fflush(stderr)

// Register our view controller class before starting the service
// This also initializes ServiceReactNativeHost early to set up pendingTabId listener
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
