import SwiftUI
import UniformTypeIdentifiers

/// DevTools panel for Metro bundler management
struct MetroPanel: View {
    @ObservedObject private var metro = MetroManager.shared

    @State private var showAddFolderSheet = false
    @State private var manualFolderPath = ""

    var body: some View {
        HSplitView {
            // Left: Controls and Watch Folders
            controlsPanel
                .frame(minWidth: 250, maxWidth: 350)

            // Right: Logs
            logsPanel
        }
    }

    // MARK: - Controls Panel

    private var controlsPanel: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Status and Controls
            statusSection
                .padding()

            Divider()

            // Watch Folders
            watchFoldersSection
        }
        .background(Color(nsColor: .controlBackgroundColor))
    }

    private var statusSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            // Status indicator
            HStack(spacing: 8) {
                Circle()
                    .fill(statusColor)
                    .frame(width: 10, height: 10)

                Text(statusText)
                    .font(.headline)

                Spacer()
            }

            // Address info
            if metro.state.isRunning, let host = metro.host {
                HStack {
                    Text("http://\(host)")
                        .font(.system(.caption, design: .monospaced))
                        .foregroundColor(.secondary)

                    Button {
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString("http://\(host)", forType: .string)
                    } label: {
                        Image(systemName: "doc.on.doc")
                    }
                    .buttonStyle(.borderless)
                    .help("Copy URL")
                }
            }

            // Control buttons
            HStack(spacing: 8) {
                if metro.state.isRunning {
                    Button("Stop") {
                        metro.stop()
                    }
                    .buttonStyle(.bordered)

                    Button("Reload") {
                        Task {
                            try? await metro.reload()
                        }
                    }
                    .buttonStyle(.bordered)

                    Button {
                        Task {
                            try? await metro.triggerHotReload()
                        }
                    } label: {
                        Image(systemName: "arrow.clockwise")
                    }
                    .buttonStyle(.bordered)
                    .help("Hot Reload")
                } else {
                    Button("Start Metro") {
                        Task {
                            try? await metro.start()
                        }
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(metro.state == .starting)
                }

                Spacer()

                Button {
                    copyAllLogs()
                } label: {
                    Image(systemName: "doc.on.doc")
                }
                .buttonStyle(.bordered)
                .help("Copy all logs")
                .disabled(metro.logs.isEmpty)

                Button("Clear") {
                    metro.clearLogs()
                }
                .buttonStyle(.bordered)
                .disabled(metro.logs.isEmpty)
            }
        }
    }

    private var statusColor: Color {
        switch metro.state {
        case .stopped: return .gray
        case .starting: return .yellow
        case .running: return .green
        case .error: return .red
        }
    }

    private var statusText: String {
        switch metro.state {
        case .stopped: return "Stopped"
        case .starting: return "Starting..."
        case .running: return "Running"
        case .error(let msg): return "Error: \(msg)"
        }
    }

    // MARK: - Watch Folders Section

    private var watchFoldersSection: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Watch Folders")
                    .font(.headline)
                    .padding(.horizontal)
                    .padding(.vertical, 8)

                Spacer()

                Menu {
                    Button("Browse...") {
                        browseForFolder()
                    }
                    Button("Enter Path...") {
                        showAddFolderSheet = true
                    }
                } label: {
                    Image(systemName: "plus")
                }
                .menuStyle(.borderlessButton)
                .padding(.trailing)
            }

            Divider()

            // Content area with centered drop hint
            ZStack {
                // Centered drop hint (behind folder list)
                VStack {
                    Spacer()
                    Text("Drag folders here to add")
                        .font(.caption)
                        .foregroundColor(.secondary)
                    Spacer()
                }

                // Folder list (top-aligned)
                VStack(spacing: 0) {
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 0) {
                            ForEach(metro.watchFolders) { folder in
                                watchFolderRow(folder)
                            }
                        }
                    }
                    Spacer(minLength: 0)
                }
            }
            .frame(maxHeight: .infinity)
        }
        .onDrop(of: [.fileURL], isTargeted: nil) { providers in
            handleDrop(providers: providers)
        }
        .sheet(isPresented: $showAddFolderSheet) {
            addFolderSheet
        }
    }

    private func watchFolderRow(_ folder: MetroManager.WatchFolder) -> some View {
        HStack {
            Image(systemName: folder.isSystemApp ? "folder.badge.gearshape" : "folder")
                .foregroundColor(folder.isSystemApp ? .blue : .secondary)

            VStack(alignment: .leading, spacing: 2) {
                Text(folder.name)
                    .font(.system(.body, design: .monospaced))

                Text(folder.path)
                    .font(.caption)
                    .foregroundColor(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }

            Spacer()

            if !folder.isSystemApp {
                Button {
                    metro.removeWatchFolder(id: folder.id)
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundColor(.secondary)
                }
                .buttonStyle(.borderless)
            }
        }
        .padding(.horizontal)
        .padding(.vertical, 6)
        .contentShape(Rectangle())
    }

    private var addFolderSheet: some View {
        VStack(spacing: 16) {
            Text("Add Watch Folder")
                .font(.headline)

            TextField("Folder path", text: $manualFolderPath)
                .textFieldStyle(.roundedBorder)
                .frame(width: 400)

            HStack {
                Button("Cancel") {
                    showAddFolderSheet = false
                    manualFolderPath = ""
                }
                .keyboardShortcut(.cancelAction)

                Button("Add") {
                    if !manualFolderPath.isEmpty {
                        metro.addWatchFolder(path: manualFolderPath)
                    }
                    showAddFolderSheet = false
                    manualFolderPath = ""
                }
                .keyboardShortcut(.defaultAction)
                .disabled(manualFolderPath.isEmpty)
            }
        }
        .padding()
    }

    // MARK: - Logs Panel

    private var logsPanel: some View {
        MetroLogsTextView(logs: metro.logs)
            .background(Color(nsColor: .textBackgroundColor))
    }

    private func copyAllLogs() {
        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm:ss"

        let text = metro.logs.map { entry in
            "[\(formatter.string(from: entry.timestamp))] \(entry.message)"
        }.joined(separator: "\n")

        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }

    // MARK: - Actions

    private func browseForFolder() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        panel.message = "Select a React Native project folder to watch"

        if panel.runModal() == .OK, let url = panel.url {
            metro.addWatchFolder(path: url.path)
        }
    }

    private func handleDrop(providers: [NSItemProvider]) -> Bool {
        for provider in providers {
            if provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) {
                provider.loadItem(forTypeIdentifier: UTType.fileURL.identifier, options: nil) { item, _ in
                    guard let data = item as? Data,
                          let url = URL(dataRepresentation: data, relativeTo: nil),
                          url.hasDirectoryPath else { return }

                    DispatchQueue.main.async {
                        metro.addWatchFolder(path: url.path)
                    }
                }
            }
        }
        return true
    }
}

// MARK: - NSTextView-based Logs View (supports selection and Cmd-C)

private struct MetroLogsTextView: NSViewRepresentable {
    let logs: [MetroManager.LogEntry]

    func makeNSView(context: Context) -> NSScrollView {
        let scrollView = NSScrollView()
        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = false
        scrollView.autohidesScrollers = true
        scrollView.borderType = .noBorder

        let textView = NSTextView()
        textView.isEditable = false
        textView.isSelectable = true
        textView.backgroundColor = .textBackgroundColor
        textView.font = NSFont.monospacedSystemFont(ofSize: 11, weight: .regular)
        textView.textContainerInset = NSSize(width: 8, height: 4)
        textView.autoresizingMask = [.width]
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.textContainer?.widthTracksTextView = true
        textView.textContainer?.containerSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)

        scrollView.documentView = textView
        context.coordinator.textView = textView
        context.coordinator.scrollView = scrollView

        return scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        guard let textView = scrollView.documentView as? NSTextView else { return }

        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm:ss"

        let attributedString = NSMutableAttributedString()
        let timestampColor = NSColor.secondaryLabelColor
        let normalColor = NSColor.labelColor
        let errorColor = NSColor.systemRed
        let font = NSFont.monospacedSystemFont(ofSize: 11, weight: .regular)

        for (index, entry) in logs.enumerated() {
            // Timestamp
            let timestamp = "[\(formatter.string(from: entry.timestamp))] "
            let timestampAttr = NSAttributedString(string: timestamp, attributes: [
                .foregroundColor: timestampColor,
                .font: font
            ])
            attributedString.append(timestampAttr)

            // Message
            let message = entry.message + (index < logs.count - 1 ? "\n" : "")
            let messageAttr = NSAttributedString(string: message, attributes: [
                .foregroundColor: entry.isError ? errorColor : normalColor,
                .font: font
            ])
            attributedString.append(messageAttr)
        }

        // Only update if content changed
        let currentText = textView.attributedString().string
        let newText = attributedString.string
        if currentText != newText {
            let wasAtBottom = context.coordinator.isScrolledToBottom(scrollView)
            textView.textStorage?.setAttributedString(attributedString)

            // Auto-scroll to bottom if was at bottom
            if wasAtBottom {
                textView.scrollToEndOfDocument(nil)
            }
        }
    }

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    class Coordinator {
        weak var textView: NSTextView?
        weak var scrollView: NSScrollView?

        func isScrolledToBottom(_ scrollView: NSScrollView) -> Bool {
            guard let documentView = scrollView.documentView else { return true }
            let visibleRect = scrollView.contentView.bounds
            let documentHeight = documentView.frame.height
            // Consider "at bottom" if within 50 points
            return visibleRect.maxY >= documentHeight - 50
        }
    }
}

#Preview {
    MetroPanel()
        .frame(width: 800, height: 500)
}
