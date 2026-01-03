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
            // Header - Safari style: centered title, Clear button on right
            HStack {
                Spacer()
                Text("Downloads")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundColor(.primary)
                Spacer()
            }
            .overlay(alignment: .trailing) {
                if !manager.downloads.isEmpty {
                    Button("Clear") {
                        manager.clearCompleted()
                    }
                    .font(.system(size: 12))
                    .foregroundColor(.primary)
                    .padding(.horizontal, 10)
                    .padding(.vertical, 4)
                    .background(Color.primary.opacity(0.1))
                    .cornerRadius(4)
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)

            // Download list
            if manager.downloads.isEmpty {
                VStack(spacing: 8) {
                    Image(systemName: "arrow.down.circle")
                        .font(.system(size: 32))
                        .foregroundColor(.secondary)
                    Text("No Downloads")
                        .font(.system(size: 12))
                        .foregroundColor(.secondary)
                }
                .frame(maxWidth: .infinity, minHeight: 80)
                .padding()
            } else {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(manager.downloads) { item in
                            DownloadItemRow(item: item)
                        }
                    }
                }
                .frame(maxHeight: 300)
            }
        }
        .frame(width: 340)
        .background(Color(nsColor: NSColor(white: 0.2, alpha: 0.95)))
    }
}

// MARK: - Download Item Row

struct DownloadItemRow: View {
    @ObservedObject var item: DownloadItem
    @State private var isHovering = false

    var body: some View {
        HStack(spacing: 12) {
            // File icon - Safari style with download badge
            ZStack(alignment: .bottomTrailing) {
                fileIcon
                    .frame(width: 32, height: 40)

                // Download badge for active downloads
                if item.state == .downloading || item.state == .waiting {
                    Image(systemName: "arrow.down.circle.fill")
                        .font(.system(size: 12))
                        .foregroundColor(.accentColor)
                        .background(Circle().fill(Color(nsColor: .windowBackgroundColor)).frame(width: 14, height: 14))
                }
            }

            // File info
            VStack(alignment: .leading, spacing: 2) {
                Text(item.filename)
                    .font(.system(size: 13))
                    .foregroundColor(.primary)
                    .lineLimit(1)
                    .truncationMode(.middle)

                // Progress or status - Safari style
                switch item.state {
                case .waiting:
                    Text("Waiting for permission")
                        .font(.system(size: 11))
                        .foregroundColor(.secondary)

                case .downloading:
                    HStack(spacing: 4) {
                        Text(item.progressText)
                            .font(.system(size: 11))
                            .foregroundColor(.secondary)
                    }

                case .completed:
                    Text("Completed")
                        .font(.system(size: 11))
                        .foregroundColor(.secondary)

                case .failed:
                    Text(item.error ?? "Download failed")
                        .font(.system(size: 11))
                        .foregroundColor(.red)
                        .lineLimit(2)
                        .fixedSize(horizontal: false, vertical: true)

                case .cancelled:
                    Text("Cancelled")
                        .font(.system(size: 11))
                        .foregroundColor(.secondary)
                }

                // Progress bar for active downloads
                if item.state == .downloading {
                    ProgressView(value: item.progress)
                        .progressViewStyle(.linear)
                        .tint(.accentColor)
                }
            }

            Spacer()

            // Action buttons - Safari style
            HStack(spacing: 6) {
                if item.state == .failed {
                    // Retry button (orange circle arrow)
                    Button(action: { item.retry() }) {
                        Image(systemName: "arrow.clockwise.circle.fill")
                            .font(.system(size: 20))
                            .foregroundColor(.orange)
                    }
                    .buttonStyle(.plain)
                    .help("Retry")
                }

                if item.state == .downloading || item.state == .waiting {
                    // Cancel button
                    Button(action: { item.cancel() }) {
                        Image(systemName: "xmark.circle.fill")
                            .font(.system(size: 20))
                            .foregroundColor(.secondary)
                    }
                    .buttonStyle(.plain)
                    .help("Cancel")
                }

                // Show in Finder button (always visible for completed/failed)
                if item.state == .completed || item.state == .failed {
                    Button(action: { DownloadManager.shared.revealInFinder(item) }) {
                        Image(systemName: "magnifyingglass.circle.fill")
                            .font(.system(size: 20))
                            .foregroundColor(.secondary)
                    }
                    .buttonStyle(.plain)
                    .help("Show in Finder")
                }
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(isHovering ? Color.white.opacity(0.05) : Color.clear)
        .onHover { hovering in
            isHovering = hovering
        }
        .onTapGesture(count: 2) {
            if item.state == .completed {
                DownloadManager.shared.openFile(item)
            }
        }
    }

    private var fileIcon: some View {
        let ext = (item.filename as NSString).pathExtension.lowercased()

        // Use system file icon representation
        return Group {
            switch ext {
            case "dmg", "pkg":
                Image(systemName: "externaldrive.badge.arrow.down")
                    .font(.system(size: 28))
                    .foregroundColor(.secondary)
            case "zip", "rar", "7z", "tar", "gz":
                Image(systemName: "doc.zipper")
                    .font(.system(size: 28))
                    .foregroundColor(.secondary)
            case "pdf":
                Image(systemName: "doc.richtext")
                    .font(.system(size: 28))
                    .foregroundColor(.red)
            case "jpg", "jpeg", "png", "gif", "webp", "heic":
                Image(systemName: "photo")
                    .font(.system(size: 28))
                    .foregroundColor(.secondary)
            case "mp4", "mov", "avi", "mkv":
                Image(systemName: "film")
                    .font(.system(size: 28))
                    .foregroundColor(.secondary)
            case "mp3", "wav", "aac", "flac", "m4a":
                Image(systemName: "music.note")
                    .font(.system(size: 28))
                    .foregroundColor(.pink)
            default:
                Image(systemName: "doc.fill")
                    .font(.system(size: 28))
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
        Button(action: { showPopover.toggle() }) {
            Image(systemName: "arrow.down.circle")
                .font(.system(size: 16))
                .foregroundColor(manager.hasActiveDownloads ? .accentColor : .primary.opacity(0.7))
        }
        .buttonStyle(.plain)
        .popover(isPresented: $showPopover, arrowEdge: .bottom) {
            DownloadPopoverView()
        }
        .help("Downloads")
    }
}

#Preview {
    DownloadPopoverView()
        .preferredColorScheme(.dark)
}
