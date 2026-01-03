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
            // Header
            HStack {
                Text("Downloads")
                    .font(.headline)
                Spacer()
                if !manager.downloads.isEmpty {
                    Button("Clear") {
                        manager.clearCompleted()
                    }
                    .buttonStyle(.plain)
                    .foregroundColor(.accentColor)
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)

            Divider()

            // Download list
            if manager.downloads.isEmpty {
                VStack(spacing: 8) {
                    Image(systemName: "arrow.down.circle")
                        .font(.system(size: 32))
                        .foregroundColor(.secondary)
                    Text("No Downloads")
                        .font(.subheadline)
                        .foregroundColor(.secondary)
                }
                .frame(maxWidth: .infinity, minHeight: 100)
                .padding()
            } else {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(manager.downloads) { item in
                            DownloadItemRow(item: item)
                            if item.id != manager.downloads.last?.id {
                                Divider()
                                    .padding(.leading, 48)
                            }
                        }
                    }
                }
                .frame(maxHeight: 300)
            }
        }
        .frame(width: 320)
    }
}

// MARK: - Download Item Row

struct DownloadItemRow: View {
    @ObservedObject var item: DownloadItem
    @State private var isHovering = false

    var body: some View {
        HStack(spacing: 12) {
            // File icon
            fileIcon
                .frame(width: 32, height: 32)

            // File info
            VStack(alignment: .leading, spacing: 4) {
                Text(item.filename)
                    .font(.system(size: 13))
                    .lineLimit(1)
                    .truncationMode(.middle)

                // Progress or status
                switch item.state {
                case .waiting:
                    Text("Waiting...")
                        .font(.system(size: 11))
                        .foregroundColor(.secondary)

                case .downloading:
                    VStack(alignment: .leading, spacing: 2) {
                        ProgressView(value: item.progress)
                            .progressViewStyle(.linear)
                        Text(item.progressText)
                            .font(.system(size: 11))
                            .foregroundColor(.secondary)
                    }

                case .completed:
                    Text("Completed")
                        .font(.system(size: 11))
                        .foregroundColor(.secondary)

                case .failed:
                    Text(item.error ?? "Failed")
                        .font(.system(size: 11))
                        .foregroundColor(.red)

                case .cancelled:
                    Text("Cancelled")
                        .font(.system(size: 11))
                        .foregroundColor(.secondary)
                }
            }

            Spacer()

            // Actions
            HStack(spacing: 8) {
                if item.state == .downloading || item.state == .waiting {
                    Button(action: { item.cancel() }) {
                        Image(systemName: "xmark.circle.fill")
                            .font(.system(size: 16))
                            .foregroundColor(.secondary)
                    }
                    .buttonStyle(.plain)
                } else if item.state == .completed {
                    Button(action: { DownloadManager.shared.revealInFinder(item) }) {
                        Image(systemName: "magnifyingglass.circle.fill")
                            .font(.system(size: 16))
                            .foregroundColor(.secondary)
                    }
                    .buttonStyle(.plain)
                    .help("Show in Finder")
                }
            }
            .opacity(isHovering ? 1 : 0.5)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(isHovering ? Color.primary.opacity(0.05) : Color.clear)
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
        let iconName: String

        switch ext {
        case "pdf":
            iconName = "doc.fill"
        case "zip", "rar", "7z", "tar", "gz":
            iconName = "doc.zipper"
        case "dmg", "pkg":
            iconName = "externaldrive.fill"
        case "jpg", "jpeg", "png", "gif", "webp", "heic":
            iconName = "photo.fill"
        case "mp4", "mov", "avi", "mkv":
            iconName = "film.fill"
        case "mp3", "wav", "aac", "flac":
            iconName = "music.note"
        case "doc", "docx":
            iconName = "doc.text.fill"
        case "xls", "xlsx":
            iconName = "tablecells.fill"
        case "ppt", "pptx":
            iconName = "rectangle.fill.on.rectangle.fill"
        default:
            iconName = "doc.fill"
        }

        return Image(systemName: iconName)
            .font(.system(size: 24))
            .foregroundColor(.secondary)
    }
}

// MARK: - Download Toolbar Button

struct DownloadToolbarButton: View {
    @ObservedObject var manager = DownloadManager.shared
    @State private var showPopover = false

    var body: some View {
        Button(action: { showPopover.toggle() }) {
            ZStack {
                Image(systemName: "arrow.down.circle")
                    .font(.system(size: 16))
                    .foregroundColor(manager.hasActiveDownloads ? .accentColor : .white.opacity(0.7))

                // Active download indicator
                if manager.hasActiveDownloads {
                    Circle()
                        .fill(Color.accentColor)
                        .frame(width: 8, height: 8)
                        .offset(x: 8, y: -8)
                }
            }
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
}
