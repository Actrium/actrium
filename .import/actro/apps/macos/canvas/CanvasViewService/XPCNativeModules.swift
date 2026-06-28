//
//  XPCNativeModules.swift
//  CanvasViewService
//
//  Native modules for React Native running in XPC service.
//  Uses DistributedNotificationCenter to communicate with host process.
//

import Foundation
import React

// MARK: - StartPage Module

/// Native module for Start Page actions from React Native (XPC Service version)
@objc(StartPageModule)
class StartPageModule: NSObject {

    @objc
    func openURL(_ urlString: String?) {
        guard let urlString = urlString else {
            NSLog("[StartPageModule] URL is nil")
            return
        }

        NSLog("[StartPageModule] Opening URL: \(urlString)")

        // Send to host process via DistributedNotificationCenter
        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name("com.canvas.openURLFromStartPage"),
            object: nil,
            userInfo: ["url": urlString],
            deliverImmediately: true
        )
    }

    @objc
    static func requiresMainQueueSetup() -> Bool {
        return false
    }

    @objc
    static func moduleName() -> String! {
        return "StartPageModule"
    }
}

// MARK: - History Module

/// Native module for history actions from React Native (XPC Service version)
@objc(HistoryModule)
class HistoryModule: NSObject {

    @objc
    func openHistoryItem(_ itemId: String, _ appURL: String?) {
        guard let urlString = appURL else {
            NSLog("[HistoryModule] URL is nil")
            return
        }

        NSLog("[HistoryModule] Opening history item: \(itemId), url: \(urlString)")

        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name("com.canvas.openHistoryItem"),
            object: nil,
            userInfo: ["itemId": itemId, "url": urlString],
            deliverImmediately: true
        )
    }

    @objc
    func deleteHistoryItem(_ itemId: String) {
        NSLog("[HistoryModule] Delete history item: \(itemId)")

        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name("com.canvas.deleteHistoryItem"),
            object: nil,
            userInfo: ["itemId": itemId],
            deliverImmediately: true
        )
    }

    @objc
    func restoreGroup(_ groupId: String) {
        NSLog("[HistoryModule] Restore group: \(groupId)")

        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name("com.canvas.restoreHistoryGroup"),
            object: nil,
            userInfo: ["groupId": groupId],
            deliverImmediately: true
        )
    }

    @objc
    func clearHistory() {
        NSLog("[HistoryModule] Clear all history")

        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name("com.canvas.clearHistory"),
            object: nil,
            userInfo: nil,
            deliverImmediately: true
        )
    }

    @objc
    static func requiresMainQueueSetup() -> Bool {
        return false
    }

    @objc
    static func moduleName() -> String! {
        return "HistoryModule"
    }
}

// MARK: - Bookmarks Module

/// Native module for bookmark actions from React Native (XPC Service version)
@objc(BookmarksModule)
class BookmarksModule: NSObject {

    @objc
    func openBookmark(_ bookmarkId: String, _ appURL: String?) {
        guard let urlString = appURL else {
            NSLog("[BookmarksModule] URL is nil")
            return
        }

        NSLog("[BookmarksModule] Opening bookmark: \(bookmarkId), url: \(urlString)")

        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name("com.canvas.openBookmark"),
            object: nil,
            userInfo: ["bookmarkId": bookmarkId, "url": urlString],
            deliverImmediately: true
        )
    }

    @objc
    func deleteBookmark(_ bookmarkId: String) {
        NSLog("[BookmarksModule] Delete bookmark: \(bookmarkId)")

        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name("com.canvas.deleteBookmark"),
            object: nil,
            userInfo: ["bookmarkId": bookmarkId],
            deliverImmediately: true
        )
    }

    @objc
    static func requiresMainQueueSetup() -> Bool {
        return false
    }

    @objc
    static func moduleName() -> String! {
        return "BookmarksModule"
    }
}

// MARK: - Native Logger Module

/// Native logger module for React Native (XPC Service version)
/// Logs to stderr and optionally sends to host for DevTools Console
@objc(NativeLogger)
class NativeLogger: NSObject {

    // Remote log server for development
    private static let logServerURL = "http://localhost:9999/log"

    /// Write to shared IPC debug log file for cross-process log correlation
    private static func writeToIPCLog(_ message: String) {
        let timestamp: String = {
            let now = Date()
            let formatter = DateFormatter()
            formatter.dateFormat = "HH:mm:ss.SSS"
            return formatter.string(from: now)
        }()
        let line = "[\(timestamp)] [JS] \(message)\n"

        // Write to shared IPC log file
        let logPath = "/tmp/canvas_ipc_debug.log"
        if let handle = FileHandle(forWritingAtPath: logPath) {
            handle.seekToEndOfFile()
            if let data = line.data(using: .utf8) {
                handle.write(data)
            }
            handle.closeFile()
        } else {
            FileManager.default.createFile(atPath: logPath, contents: line.data(using: .utf8))
        }
    }

    private static func sendToHost(level: String, source: String, message: String) {
        // Send log to host process for DevTools Console
        DistributedNotificationCenter.default().postNotificationName(
            NSNotification.Name("com.canvas.xpcLog"),
            object: nil,
            userInfo: [
                "level": level,
                "source": source,
                "message": message
            ],
            deliverImmediately: true
        )
    }

    private static func sendToRemote(level: String, message: String) {
        // Disabled: Remote log server causes "Connection refused" noise when not running
        // The DevTools Console receives logs via DistributedNotificationCenter (sendToHost)
        // which is more reliable and doesn't require a separate server process
    }

    @objc
    func log(_ message: String) {
        NSLog("[JS] %@", message)
        NativeLogger.writeToIPCLog(message)
        NativeLogger.sendToHost(level: "info", source: "JS", message: message)
        NativeLogger.sendToRemote(level: "log", message: message)
    }

    @objc
    func warn(_ message: String) {
        NSLog("[JS WARN] %@", message)
        NativeLogger.sendToHost(level: "warn", source: "JS", message: message)
        NativeLogger.sendToRemote(level: "warn", message: message)
    }

    @objc
    func error(_ message: String) {
        NSLog("[JS ERROR] %@", message)
        NativeLogger.sendToHost(level: "error", source: "JS", message: message)
        NativeLogger.sendToRemote(level: "error", message: message)
    }

    @objc
    static func nativeLog(_ message: String) {
        NSLog("[Native] %@", message)
        sendToHost(level: "info", source: "Native", message: message)
        sendToRemote(level: "log", message: "[Native] \(message)")
    }

    @objc
    static func nativeWarn(_ message: String) {
        NSLog("[Native WARN] %@", message)
        sendToHost(level: "warn", source: "Native", message: message)
        sendToRemote(level: "warn", message: "[Native] \(message)")
    }

    @objc
    static func nativeError(_ message: String) {
        NSLog("[Native ERROR] %@", message)
        sendToHost(level: "error", source: "Native", message: message)
        sendToRemote(level: "error", message: "[Native] \(message)")
    }

    @objc
    static func requiresMainQueueSetup() -> Bool {
        return false
    }

    @objc
    static func moduleName() -> String! {
        return "NativeLogger"
    }
}
