//
//  DownloadPopoverView.swift
//  Canvas
//
//  Safari 同款下载弹出面板
//

import SwiftUI

// MARK: - Download Popover View

struct DownloadPopoverView: View {
    @ObservedObject var manager = DownloadManager.shared

    var body: some View {
        VStack(spacing: 0) {
            // Header with Clear button
            HStack {
                Text("Downloads")
                    .font(.headline)

                Spacer()

                if manager.downloads.contains(where: { $0.state == .completed || $0.state == .failed || $0.state == .cancelled }) {
                    Button("Clear") {
                        manager.clearCompleted()
                    }
                    .buttonStyle(.borderless)
                    .font(.subheadline)
                }
            }
            .padding(.horizontal, 12)
            .padding(.top, 12)
            .padding(.bottom, 8)

            Divider()

            // Download list
            ScrollView {
                LazyVStack(spacing: 0) {
                    ForEach(manager.downloads) { item in
                        DownloadItemRow(item: item)
                        if item.id != manager.downloads.last?.id {
                            Divider()
                                .padding(.leading, 52)
                        }
                    }
                }
            }
            .frame(maxHeight: 280)
        }
        .frame(width: 300)
        .onAppear {
            // Check if downloaded files still exist when popover opens
            manager.checkAllFilesExist()
        }
    }
}

// MARK: - Download Item Row

struct DownloadItemRow: View {
    @ObservedObject var item: DownloadItem
    @State private var isHovering = false

    /// Whether this item should be disabled (file deleted)
    private var isDisabled: Bool {
        item.state == .completed && !item.fileExists
    }

    var body: some View {
        HStack(spacing: 10) {
            // File icon
            fileIcon
                .frame(width: 32, height: 32)

            // File info
            VStack(alignment: .leading, spacing: 2) {
                Text(item.filename)
                    .font(.system(size: 12, weight: .medium))
                    .lineLimit(1)
                    .truncationMode(.middle)

                // Status text
                statusText
                    .font(.system(size: 11))
                    .foregroundColor(.secondary)

                // Progress bar for active downloads
                if item.state == .downloading {
                    ProgressView(value: item.progress)
                        .progressViewStyle(.linear)
                        .controlSize(.small)
                }
            }

            Spacer(minLength: 4)

            // Action button
            actionButton
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(isHovering && !isDisabled ? Color.primary.opacity(0.05) : Color.clear)
        .contentShape(Rectangle())
        .onHover { isHovering = $0 }
        .onTapGesture(count: 2) {
            if item.state == .completed && item.fileExists {
                DownloadManager.shared.openFile(item)
            }
        }
        .opacity(isDisabled ? 0.5 : 1.0)
    }

    @ViewBuilder
    private var statusText: some View {
        switch item.state {
        case .waiting:
            Text("Waiting...")
        case .downloading:
            Text(item.progressText)
        case .completed:
            if item.fileExists {
                Text("Completed")
            } else {
                Text("File Deleted")
                    .foregroundColor(.secondary)
            }
        case .failed:
            Text(item.error ?? "Failed")
                .foregroundColor(.red)
        case .cancelled:
            Text("Cancelled")
        }
    }

    @ViewBuilder
    private var actionButton: some View {
        switch item.state {
        case .downloading, .waiting:
            Button(action: { item.cancel() }) {
                Image(systemName: "xmark.circle.fill")
                    .foregroundColor(.secondary)
            }
            .buttonStyle(.plain)
            .help("Cancel")

        case .failed:
            Button(action: { item.retry() }) {
                Image(systemName: "arrow.clockwise.circle.fill")
                    .foregroundColor(.orange)
            }
            .buttonStyle(.plain)
            .help("Retry")

        case .completed:
            if item.fileExists {
                Button(action: { DownloadManager.shared.revealInFinder(item) }) {
                    Image(systemName: "magnifyingglass.circle.fill")
                        .foregroundColor(.secondary)
                }
                .buttonStyle(.plain)
                .help("Show in Finder")
            } else {
                // File deleted - no action available
                EmptyView()
            }

        case .cancelled:
            EmptyView()
        }
    }

    private var fileIcon: some View {
        let ext = (item.filename as NSString).pathExtension.lowercased()

        return Group {
            switch ext {
            case "dmg", "pkg":
                Image(systemName: "externaldrive.badge.arrow.down")
                    .font(.system(size: 20))
                    .foregroundColor(.purple)
            case "zip", "rar", "7z", "tar", "gz":
                Image(systemName: "doc.zipper")
                    .font(.system(size: 20))
                    .foregroundColor(.secondary)
            case "pdf":
                Image(systemName: "doc.richtext")
                    .font(.system(size: 20))
                    .foregroundColor(.red)
            case "jpg", "jpeg", "png", "gif", "webp", "heic", "svg":
                Image(systemName: "photo")
                    .font(.system(size: 20))
                    .foregroundColor(.cyan)
            case "mp4", "mov", "avi", "mkv", "webm":
                Image(systemName: "film")
                    .font(.system(size: 20))
                    .foregroundColor(.pink)
            case "mp3", "wav", "aac", "flac", "m4a":
                Image(systemName: "music.note")
                    .font(.system(size: 20))
                    .foregroundColor(.pink)
            default:
                Image(systemName: "doc.fill")
                    .font(.system(size: 20))
                    .foregroundColor(.secondary)
            }
        }
    }
}

// MARK: - Download Toolbar Button

struct DownloadToolbarButton: View {
    @ObservedObject var manager = DownloadManager.shared
    @State private var showPopover = false

    var body: some View {
        // Only show when there are downloads (active or recent)
        if !manager.downloads.isEmpty {
            Button(action: { showPopover.toggle() }) {
                Image(systemName: manager.hasActiveDownloads ? "arrow.down.circle.fill" : "arrow.down.circle")
            }
            .popover(isPresented: $showPopover, arrowEdge: .bottom) {
                DownloadPopoverView()
            }
            .help("Downloads")
        }
    }
}

#Preview {
    DownloadPopoverView()
        .preferredColorScheme(.dark)
}
