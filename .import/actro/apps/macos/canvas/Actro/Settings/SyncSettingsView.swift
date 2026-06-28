import SwiftUI

struct SyncSettingsView: View {
    @StateObject private var syncManager = SyncManager.shared

    var body: some View {
        Form {
            // MARK: - Sync Folder
            Section {
                if let url = syncManager.syncFolderURL {
                    LabeledContent("Folder") {
                        HStack(spacing: 8) {
                            Text(url.path)
                                .foregroundColor(.secondary)
                                .lineLimit(1)
                                .truncationMode(.middle)
                            Button("Remove") {
                                syncManager.removeSyncFolder()
                            }
                        }
                    }
                } else {
                    LabeledContent("Folder") {
                        Button("Choose Folder...") {
                            syncManager.selectSyncFolder()
                        }
                    }
                }
            } header: {
                Text("Sync Folder")
            } footer: {
                Text("Select a folder (e.g., iCloud Drive or Dropbox) to sync data across devices.")
            }

            // MARK: - Status
            Section("Status") {
                if let date = syncManager.lastSyncDate {
                    LabeledContent("Last Sync") {
                        Text(date, style: .relative)
                            .foregroundColor(.secondary)
                    }
                } else {
                    LabeledContent("Last Sync") {
                        Text("Never")
                            .foregroundColor(.secondary)
                    }
                }

                if let error = syncManager.syncError {
                    LabeledContent("Error") {
                        Text(error)
                            .foregroundColor(.red)
                            .lineLimit(2)
                    }
                }
            }

            // MARK: - Content
            Section("Content") {
                Toggle("Bookmarks", isOn: $syncManager.syncBookmarks)
                Toggle("Tags", isOn: $syncManager.syncTags)
                Toggle("Vault", isOn: $syncManager.syncVault)
                Toggle("Settings", isOn: $syncManager.syncSettings)
            }

            // MARK: - Auto Sync
            Section {
                Toggle("Auto-sync on file changes", isOn: $syncManager.autoSyncEnabled)
                    .onChange(of: syncManager.autoSyncEnabled) {
                        if syncManager.autoSyncEnabled && syncManager.syncFolderURL != nil {
                            syncManager.startMonitoring()
                        } else {
                            syncManager.stopMonitoring()
                        }
                    }
            } header: {
                Text("Automatic")
            }

            // MARK: - Actions
            Section("Actions") {
                HStack {
                    Button("Export Now") {
                        syncManager.exportNow()
                    }
                    .disabled(syncManager.syncFolderURL == nil || syncManager.isSyncing)

                    Button("Import Now") {
                        syncManager.importNow()
                    }
                    .disabled(syncManager.syncFolderURL == nil || syncManager.isSyncing)

                    if syncManager.isSyncing {
                        ProgressView()
                            .controlSize(.small)
                    }
                }
            }
        }
        .formStyle(.grouped)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
