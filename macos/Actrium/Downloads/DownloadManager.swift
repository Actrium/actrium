//
//  DownloadManager.swift
//  Actrium
//
//  Download Manager - Safari Download function for the same paragraph
//

import Foundation
import WebKit
import AppKit
import Combine
import SwiftUI

// MARK: - Download Item

/// Individual Download Items
class DownloadItem: ObservableObject, Identifiable {
    let id = UUID()
    let filename: String
    let sourceURL: URL?
    var destinationURL: URL?

    @Published var state: DownloadState = .waiting
    @Published var bytesReceived: Int64 = 0
    @Published var totalBytes: Int64 = 0
    @Published var error: String?
    @Published var fileExists: Bool = true

    var download: WKDownload?

    /// Check if the downloaded file still exists on disk
    func checkFileExists() {
        guard state == .completed, let url = destinationURL else {
            fileExists = true  // Not applicable for non-completed items
            return
        }
        fileExists = FileManager.default.fileExists(atPath: url.path)
    }

    var progress: Double {
        guard totalBytes > 0 else { return 0 }
        return Double(bytesReceived) / Double(totalBytes)
    }

    var progressText: String {
        if totalBytes > 0 {
            let received = ByteCountFormatter.string(fromByteCount: bytesReceived, countStyle: .file)
            let total = ByteCountFormatter.string(fromByteCount: totalBytes, countStyle: .file)
            return "\(received) of \(total)"
        } else if bytesReceived > 0 {
            return ByteCountFormatter.string(fromByteCount: bytesReceived, countStyle: .file)
        }
        return ""
    }

    init(filename: String, sourceURL: URL? = nil) {
        self.filename = filename
        self.sourceURL = sourceURL
    }

    func cancel() {
        download?.cancel()
        URLSessionDownloader.shared.cancelDownload(for: self)
        state = .cancelled
    }

    func retry() {
        guard let url = sourceURL else { return }
        bytesReceived = 0
        totalBytes = 0
        error = nil
        state = .downloading
        URLSessionDownloader.shared.startDownload(url: url, item: self)
    }
}

enum DownloadState: Equatable {
    case waiting
    case downloading
    case completed
    case failed
    case cancelled
}

// MARK: - Download Manager

/// Global Download Manager
@MainActor
class DownloadManager: ObservableObject {
    static let shared = DownloadManager()

    @Published var downloads: [DownloadItem] = []
    @Published var hasActiveDownloads: Bool = false
    private var webKitDownloadHandlers: [ObjectIdentifier: DownloadDelegateHandler] = [:]

    private init() {}

    /// Add New Download
    func addDownload(filename: String, sourceURL: URL? = nil) -> DownloadItem {
        let item = DownloadItem(filename: filename, sourceURL: sourceURL)
        downloads.insert(item, at: 0)
        updateActiveState()
        return item
    }

    /// Pass. WKDownload Find Downloaded Items
    func item(for download: WKDownload) -> DownloadItem? {
        downloads.first { $0.download === download }
    }

    /// Update Download Item Status
    func updateItem(_ item: DownloadItem, state: DownloadState) {
        item.state = state
        updateActiveState()
    }

    /// Update progress
    func updateProgress(_ item: DownloadItem, bytesReceived: Int64, totalBytes: Int64) {
        item.bytesReceived = bytesReceived
        item.totalBytes = totalBytes
        if item.state != .downloading {
            item.state = .downloading
        }
    }

    /// Clear completed downloads
    func clearCompleted() {
        downloads.removeAll { $0.state == .completed || $0.state == .failed || $0.state == .cancelled }
        updateActiveState()
    }

    /// Clear All Downloads
    func clearAll() {
        // Unactive Download
        for item in downloads where item.state == .downloading || item.state == .waiting {
            item.cancel()
        }
        downloads.removeAll()
        updateActiveState()
    }

    /// Yes. Finder Show
    func revealInFinder(_ item: DownloadItem) {
        guard let url = item.destinationURL else { return }
        NSWorkspace.shared.activateFileViewerSelecting([url])
    }

    /// Open Downloaded Files
    func openFile(_ item: DownloadItem) {
        guard let url = item.destinationURL else { return }
        NSWorkspace.shared.open(url)
    }

    /// Check if all completed download files still exist
    func checkAllFilesExist() {
        for item in downloads {
            item.checkFileExists()
        }
    }

    private func updateActiveState() {
        hasActiveDownloads = downloads.contains { $0.state == .downloading || $0.state == .waiting }
    }
}

// MARK: - URLSession-Based Download Task

/// URLSession Download Task Manager - Avoid WKDownload Limits of competence
class URLSessionDownloader: NSObject, URLSessionDownloadDelegate {
    private var session: URLSession!
    private var downloadTasks: [URLSessionDownloadTask: DownloadItem] = [:]

    static let shared = URLSessionDownloader()

    override init() {
        super.init()
        let config = URLSessionConfiguration.default
        config.timeoutIntervalForResource = 3600 // 1 hour max
        session = URLSession(configuration: config, delegate: self, delegateQueue: .main)
    }

    /// Start a download using URLSession
    func startDownload(url: URL, item: DownloadItem) {
        // Let URLSession provide its native networking identity. A direct
        // downloader must not impersonate an unrelated Safari release.
        let task = session.downloadTask(with: URLRequest(url: url))
        downloadTasks[task] = item

        Task { @MainActor in
            item.state = .downloading
        }

        task.resume()
        NSLog("📥 URLSessionDownloader: Started download")
    }

    /// Cancel a download
    func cancelDownload(for item: DownloadItem) {
        if let task = downloadTasks.first(where: { $0.value === item })?.key {
            task.cancel()
            downloadTasks.removeValue(forKey: task)
        }
    }

    // MARK: - URLSessionDownloadDelegate

    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didFinishDownloadingTo location: URL) {
        guard let item = downloadTasks[downloadTask] else { return }

        // Generate destination path
        let downloadsURL = FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask).first!
        var destinationURL = downloadsURL.appendingPathComponent(item.filename)

        // Handle duplicate filenames
        var counter = 1
        let fileManager = FileManager.default
        let baseName = (item.filename as NSString).deletingPathExtension
        let ext = (item.filename as NSString).pathExtension

        while fileManager.fileExists(atPath: destinationURL.path) {
            let newName = ext.isEmpty ? "\(baseName) (\(counter))" : "\(baseName) (\(counter)).\(ext)"
            destinationURL = downloadsURL.appendingPathComponent(newName)
            counter += 1
        }

        do {
            try fileManager.moveItem(at: location, to: destinationURL)

            Task { @MainActor in
                item.destinationURL = destinationURL
                DownloadManager.shared.updateItem(item, state: .completed)
                NSLog("📥 URLSessionDownloader: Download completed")
            }
        } catch {
            Task { @MainActor in
                item.error = error.localizedDescription
                DownloadManager.shared.updateItem(item, state: .failed)
                let nsError = error as NSError
                NSLog("📥 URLSessionDownloader: Failed to move file (%@/%d)", nsError.domain, nsError.code)
            }
        }

        downloadTasks.removeValue(forKey: downloadTask)
    }

    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didWriteData bytesWritten: Int64, totalBytesWritten: Int64, totalBytesExpectedToWrite: Int64) {
        guard let item = downloadTasks[downloadTask] else { return }

        Task { @MainActor in
            DownloadManager.shared.updateProgress(item, bytesReceived: totalBytesWritten, totalBytes: totalBytesExpectedToWrite)
        }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        guard let downloadTask = task as? URLSessionDownloadTask,
              let item = downloadTasks[downloadTask],
              let error = error else { return }

        // Don't report cancellation as error
        if (error as NSError).code == NSURLErrorCancelled {
            Task { @MainActor in
                DownloadManager.shared.updateItem(item, state: .cancelled)
            }
        } else {
            Task { @MainActor in
                item.error = error.localizedDescription
                DownloadManager.shared.updateItem(item, state: .failed)
                let nsError = error as NSError
                NSLog("📥 URLSessionDownloader: Download failed (%@/%d)", nsError.domain, nsError.code)
            }
        }

        downloadTasks.removeValue(forKey: downloadTask)
    }
}

// MARK: - Download Delegate Handler

/// Downloading Proxy Processor - For WKDownload, but automatically switch to URLSession
class DownloadDelegateHandler: NSObject, WKDownloadDelegate {
    weak var item: DownloadItem?
    private let onTerminalState: (WKDownload) -> Void

    init(item: DownloadItem, onTerminalState: @escaping (WKDownload) -> Void = { _ in }) {
        self.item = item
        self.onTerminalState = onTerminalState
        super.init()
    }

    func download(_ download: WKDownload, decideDestinationUsing response: URLResponse, suggestedFilename: String, completionHandler: @escaping (URL?) -> Void) {
        // Access Downloads Folder
        let downloadsURL = FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask).first!
        var destinationURL = downloadsURL.appendingPathComponent(suggestedFilename)

        // Process rename files
        var counter = 1
        let fileManager = FileManager.default
        let baseName = (suggestedFilename as NSString).deletingPathExtension
        let ext = (suggestedFilename as NSString).pathExtension

        while fileManager.fileExists(atPath: destinationURL.path) {
            let newName = ext.isEmpty ? "\(baseName) (\(counter))" : "\(baseName) (\(counter)).\(ext)"
            destinationURL = downloadsURL.appendingPathComponent(newName)
            counter += 1
        }

        Task { @MainActor in
            item?.destinationURL = destinationURL
            item?.state = .downloading
        }

        NSLog("📥 DownloadManager: Downloading")
        completionHandler(destinationURL)
    }

    func download(_ download: WKDownload, didReceive challenge: URLAuthenticationChallenge, completionHandler: @escaping (URLSession.AuthChallengeDisposition, URLCredential?) -> Void) {
        completionHandler(.performDefaultHandling, nil)
    }

    func download(_ download: WKDownload, didWriteData bytesWritten: Int64, totalBytesWritten: Int64, totalBytesExpectedToWrite: Int64) {
        Task { @MainActor [weak self] in
            guard let item = self?.item else { return }
            DownloadManager.shared.updateProgress(item, bytesReceived: totalBytesWritten, totalBytes: totalBytesExpectedToWrite)
        }
    }

    func downloadDidFinish(_ download: WKDownload) {
        onTerminalState(download)
        Task { @MainActor [weak self] in
            guard let item = self?.item else { return }
            item.download = nil
            DownloadManager.shared.updateItem(item, state: .completed)
            NSLog("📥 DownloadManager: Download completed")
        }
    }

    func download(_ download: WKDownload, didFailWithError error: Error, resumeData: Data?) {
        onTerminalState(download)
        Task { @MainActor [weak self] in
            guard let self = self, let item = self.item else { return }

            // If WKDownload fails with permission error, try URLSession fallback
            let nsError = error as NSError
            if (nsError.domain == NSPOSIXErrorDomain && nsError.code == 1) ||
               error.localizedDescription.contains("Operation not permitted") ||
               error.localizedDescription.contains("Unenabled Operations") {

                if !self.hasTriedURLSessionFallback, let sourceURL = item.sourceURL {
                    self.hasTriedURLSessionFallback = true
                    NSLog("📥 DownloadManager: WKDownload permission denied, falling back to URLSession")

                    // Reset item state and try URLSession
                    item.bytesReceived = 0
                    item.totalBytes = 0
                    item.error = nil
                    URLSessionDownloader.shared.startDownload(url: sourceURL, item: item)
                    return
                }
            }

            item.error = error.localizedDescription
            DownloadManager.shared.updateItem(item, state: .failed)
            let nsError = error as NSError
            NSLog("📥 DownloadManager: Download failed (%@/%d)", nsError.domain, nsError.code)
        }
    }
}
