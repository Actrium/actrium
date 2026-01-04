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
            if metro.state.isRunning {
                HStack {
                    Text("http://\(metro.host)")
                        .font(.system(.caption, design: .monospaced))
                        .foregroundColor(.secondary)

                    Button {
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString("http://\(metro.host)", forType: .string)
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

            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(metro.watchFolders) { folder in
                        watchFolderRow(folder)
                    }
                }
            }
            .frame(maxHeight: .infinity)

            // Drop zone hint
            Text("Drag folders here to add")
                .font(.caption)
                .foregroundColor(.secondary)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 8)
                .background(Color(nsColor: .quaternaryLabelColor).opacity(0.3))
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
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Logs")
                    .font(.headline)

                Spacer()

                Button {
                    // Clear would require adding a method to MetroManager
                } label: {
                    Image(systemName: "trash")
                }
                .buttonStyle(.borderless)
                .disabled(metro.logs.isEmpty)
            }
            .padding(.horizontal)
            .padding(.vertical, 8)

            Divider()

            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 2) {
                        ForEach(metro.logs) { entry in
                            logEntryRow(entry)
                                .id(entry.id)
                        }
                    }
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                }
                .onChange(of: metro.logs.count) { _, _ in
                    if let lastLog = metro.logs.last {
                        withAnimation {
                            proxy.scrollTo(lastLog.id, anchor: .bottom)
                        }
                    }
                }
            }
        }
        .background(Color(nsColor: .textBackgroundColor))
    }

    private func logEntryRow(_ entry: MetroManager.LogEntry) -> some View {
        HStack(alignment: .top, spacing: 8) {
            Text(formatTime(entry.timestamp))
                .font(.system(.caption, design: .monospaced))
                .foregroundColor(.secondary)

            Text(entry.message)
                .font(.system(.caption, design: .monospaced))
                .foregroundColor(entry.isError ? .red : .primary)
                .textSelection(.enabled)
        }
    }

    private func formatTime(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm:ss"
        return formatter.string(from: date)
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

#Preview {
    MetroPanel()
        .frame(width: 800, height: 500)
}
