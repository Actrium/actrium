//
//  DownloadManager.swift
//  Canvas
//
//  下载管理器 - Safari 同款下载功能
//

import Foundation
import WebKit
import AppKit
import Combine
import SwiftUI

// MARK: - Download Item

/// 单个下载项
class DownloadItem: ObservableObject, Identifiable {
    let id = UUID()
    let filename: String
    let sourceURL: URL?
    var destinationURL: URL?

    @Published var state: DownloadState = .waiting
    @Published var bytesReceived: Int64 = 0
    @Published var totalBytes: Int64 = 0
    @Published var error: String?

    weak var download: WKDownload?

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
        state = .cancelled
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

/// 全局下载管理器
@MainActor
class DownloadManager: ObservableObject {
    static let shared = DownloadManager()

    @Published var downloads: [DownloadItem] = []
    @Published var hasActiveDownloads: Bool = false

    private init() {}

    /// 添加新下载
    func addDownload(filename: String, sourceURL: URL? = nil) -> DownloadItem {
        let item = DownloadItem(filename: filename, sourceURL: sourceURL)
        downloads.insert(item, at: 0)
        updateActiveState()
        return item
    }

    /// 通过 WKDownload 查找下载项
    func item(for download: WKDownload) -> DownloadItem? {
        downloads.first { $0.download === download }
    }

    /// 更新下载项状态
    func updateItem(_ item: DownloadItem, state: DownloadState) {
        item.state = state
        updateActiveState()
    }

    /// 更新进度
    func updateProgress(_ item: DownloadItem, bytesReceived: Int64, totalBytes: Int64) {
        item.bytesReceived = bytesReceived
        item.totalBytes = totalBytes
        if item.state != .downloading {
            item.state = .downloading
        }
    }

    /// 清除已完成的下载
    func clearCompleted() {
        downloads.removeAll { $0.state == .completed || $0.state == .failed || $0.state == .cancelled }
        updateActiveState()
    }

    /// 清除所有下载
    func clearAll() {
        // 取消活跃的下载
        for item in downloads where item.state == .downloading || item.state == .waiting {
            item.cancel()
        }
        downloads.removeAll()
        updateActiveState()
    }

    /// 在 Finder 中显示
    func revealInFinder(_ item: DownloadItem) {
        guard let url = item.destinationURL else { return }
        NSWorkspace.shared.activateFileViewerSelecting([url])
    }

    /// 打开下载的文件
    func openFile(_ item: DownloadItem) {
        guard let url = item.destinationURL else { return }
        NSWorkspace.shared.open(url)
    }

    private func updateActiveState() {
        hasActiveDownloads = downloads.contains { $0.state == .downloading || $0.state == .waiting }
    }
}

// MARK: - Download Delegate Handler

/// 下载代理处理器 - 独立于 WebViewHolder，可跟踪下载状态
class DownloadDelegateHandler: NSObject, WKDownloadDelegate {
    weak var item: DownloadItem?

    init(item: DownloadItem) {
        self.item = item
        super.init()
    }

    func download(_ download: WKDownload, decideDestinationUsing response: URLResponse, suggestedFilename: String, completionHandler: @escaping (URL?) -> Void) {
        // 获取 Downloads 文件夹
        let downloadsURL = FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask).first!
        var destinationURL = downloadsURL.appendingPathComponent(suggestedFilename)

        // 处理重名文件
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

        NSLog("📥 DownloadManager: Downloading to \(destinationURL.path)")
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
        Task { @MainActor [weak self] in
            guard let item = self?.item else { return }
            DownloadManager.shared.updateItem(item, state: .completed)
            NSLog("📥 DownloadManager: Download completed - \(item.filename)")
        }
    }

    func download(_ download: WKDownload, didFailWithError error: Error, resumeData: Data?) {
        Task { @MainActor [weak self] in
            guard let item = self?.item else { return }
            item.error = error.localizedDescription
            DownloadManager.shared.updateItem(item, state: .failed)
            NSLog("📥 DownloadManager: Download failed - \(item.filename): \(error.localizedDescription)")
        }
    }
}
