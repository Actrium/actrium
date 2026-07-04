import Combine
import SwiftUI

struct SyncSettingsView: View {
    @StateObject private var syncManager = SyncManager.shared
    @StateObject private var browserDataSyncModel = BrowserDataSyncSettingsModel()

    var body: some View {
        Form {
            // MARK: - Browser Data Sync
            Section("Browser Data Sync") {
                LabeledContent("Status") {
                    Text(browserDataSyncModel.isSyncing ? "Syncing..." : browserDataSyncModel.statusText)
                        .foregroundColor(browserDataSyncStatusColor(browserDataSyncModel.statusKind))
                }

                if let lastResultText = browserDataSyncModel.lastResultText {
                    LabeledContent("Last Result") {
                        Text(lastResultText)
                            .foregroundColor(.secondary)
                    }
                }

                HStack {
                    Button("Sync Now") {
                        Task {
                            await browserDataSyncModel.syncNow()
                        }
                    }
                    .disabled(browserDataSyncModel.isSyncing)

                    Button("Refresh Status") {
                        Task {
                            await browserDataSyncModel.refresh()
                        }
                    }
                    .disabled(browserDataSyncModel.isSyncing)

                    if browserDataSyncModel.isSyncing {
                        ProgressView()
                            .controlSize(.small)
                    }
                }

                Picker("Environment", selection: $browserDataSyncModel.credentialEnvironment) {
                    ForEach(BrowserDataSyncSettingsModel.credentialEnvironments, id: \.self) { environment in
                        Text(environment).tag(environment)
                    }
                }
                .onChange(of: browserDataSyncModel.credentialEnvironment) {
                    browserDataSyncModel.applyEnvironmentDefaultBaseUrl()
                }

                TextField("Base URL", text: $browserDataSyncModel.credentialBaseUrl)
                    .textFieldStyle(.roundedBorder)

                SecureField("Access Token", text: $browserDataSyncModel.credentialAccessToken)
                    .textFieldStyle(.roundedBorder)

                if !browserDataSyncModel.credentialDeviceId.isEmpty {
                    LabeledContent("Device") {
                        Text(browserDataSyncModel.credentialDeviceId)
                            .foregroundColor(.secondary)
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                }

                HStack {
                    Button("Save Connection") {
                        Task {
                            await browserDataSyncModel.saveCredentials()
                        }
                    }
                    .disabled(!browserDataSyncModel.canSaveCredentials)

                    Button("Clear Connection") {
                        Task {
                            await browserDataSyncModel.clearCredentials()
                        }
                    }
                    .disabled(!browserDataSyncModel.hasCredentials)
                }
            }

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
        .task {
            await browserDataSyncModel.refresh()
        }
    }

    private func browserDataSyncStatusColor(_ statusKind: String) -> Color {
        switch statusKind {
        case "pending", "retryScheduled", "waitingForCredentials":
            return .orange
        case "requiresUserAction":
            return .red
        default:
            return .secondary
        }
    }
}

@MainActor
private final class BrowserDataSyncSettingsModel: ObservableObject {
    static let credentialEnvironments = BrowserSyncCredentialsProvider.supportedEnvironments

    @Published private(set) var statusKind = "unavailable"
    @Published private(set) var statusText = "Unavailable"
    @Published private(set) var lastResultText: String?
    @Published private(set) var isSyncing = false
    @Published var credentialEnvironment = BrowserSyncCredentialsProvider.defaultEnvironment
    @Published var credentialBaseUrl = BrowserSyncCredentialsProvider.defaultBaseUrl(
        for: BrowserSyncCredentialsProvider.defaultEnvironment
    )
    @Published var credentialAccessToken = ""
    @Published private(set) var credentialDeviceId = ""
    private var lastAutoBaseUrl = BrowserSyncCredentialsProvider.defaultBaseUrl(
        for: BrowserSyncCredentialsProvider.defaultEnvironment
    )

    var canSaveCredentials: Bool {
        !credentialAccessToken.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    var hasCredentials: Bool {
        !credentialAccessToken.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            || !credentialDeviceId.isEmpty
    }

    func refresh() async {
        refreshCredentials()
        apply(statusJson: await BrowserDataRepository.shared.syncStatusJson())
    }

    func syncNow() async {
        guard !isSyncing else { return }
        isSyncing = true
        _ = await BrowserDataRepository.shared.syncNow()
        await refresh()
        isSyncing = false
    }

    func saveCredentials() async {
        let environment = normalizedEnvironment(credentialEnvironment)
        let baseUrl = normalizedBaseUrl(credentialBaseUrl, environment: environment)
        BrowserSyncCredentialsProvider.shared.saveSession(
            environment: environment,
            accessToken: credentialAccessToken,
            baseUrl: baseUrl
        )
        await BrowserDataRepository.shared.scheduleSyncAfterCredentialChange()
        await refresh()
    }

    func clearCredentials() async {
        BrowserSyncCredentialsProvider.shared.clear(environment: credentialEnvironment)
        await BrowserDataRepository.shared.cancelPendingSync()
        await refresh()
    }

    func applyEnvironmentDefaultBaseUrl() {
        let environment = normalizedEnvironment(credentialEnvironment)
        let nextBaseUrl = BrowserSyncCredentialsProvider.defaultBaseUrl(for: environment)
        let currentBaseUrl = credentialBaseUrl.trimmingCharacters(in: .whitespacesAndNewlines)
        if currentBaseUrl.isEmpty || currentBaseUrl == lastAutoBaseUrl {
            credentialBaseUrl = nextBaseUrl
        }
        lastAutoBaseUrl = nextBaseUrl
    }

    private func refreshCredentials() {
        guard let snapshot = BrowserSyncCredentialsProvider.shared.snapshot() else {
            credentialEnvironment = BrowserSyncCredentialsProvider.defaultEnvironment
            credentialBaseUrl = BrowserSyncCredentialsProvider.defaultBaseUrl(for: credentialEnvironment)
            credentialAccessToken = ""
            credentialDeviceId = ""
            lastAutoBaseUrl = credentialBaseUrl
            return
        }

        credentialEnvironment = normalizedEnvironment(snapshot.environment)
        credentialBaseUrl = normalizedBaseUrl(snapshot.baseUrl, environment: credentialEnvironment)
        credentialAccessToken = snapshot.accessToken
        credentialDeviceId = snapshot.deviceId
        lastAutoBaseUrl = BrowserSyncCredentialsProvider.defaultBaseUrl(for: credentialEnvironment)
    }

    private func apply(statusJson: String?) {
        guard let statusJson,
              let data = statusJson.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data),
              let payload = object as? [String: Any]
        else {
            statusKind = "unavailable"
            statusText = "Unavailable"
            lastResultText = nil
            return
        }

        let nextStatusKind = payload["status"] as? String ?? "unavailable"
        let nextDelay = payload["nextRunDelaySeconds"] as? NSNumber
        let nextClassification = Self.normalizedClassification(payload["lastClassification"])
        statusKind = nextStatusKind
        statusText = Self.displayStatus(
            nextStatusKind,
            delaySeconds: nextDelay?.intValue,
            classification: nextClassification
        )

        if let classification = nextClassification {
            lastResultText = Self.displayClassification(classification)
        } else {
            lastResultText = nil
        }
    }

    private static func displayStatus(_ status: String, delaySeconds: Int?, classification: String?) -> String {
        switch status {
        case "idle":
            return "Up to date"
        case "pending":
            return "Pending"
        case "retryScheduled":
            if let delaySeconds {
                return "Retry in \(delaySeconds)s"
            }
            return "Retry scheduled"
        case "waitingForCredentials":
            return "Sign in required"
        case "requiresUserAction":
            if classification == "clientUpgradeRequired" {
                return "App update required"
            }
            return "Action required"
        default:
            return "Unavailable"
        }
    }

    private static func displayClassification(_ classification: String) -> String {
        switch classification {
        case "ok", "success":
            return "Succeeded"
        case "credentialsUnavailable":
            return "Credentials unavailable"
        case "serverUnavailable":
            return "Server unavailable"
        case "clientUpgradeRequired":
            return "App update required"
        case "validationError":
            return "Validation error"
        default:
            return classification
        }
    }

    private static func normalizedClassification(_ value: Any?) -> String? {
        guard let classification = value as? String else { return nil }
        let normalized = classification.trimmingCharacters(in: .whitespacesAndNewlines)
        return normalized.isEmpty ? nil : normalized
    }

    private func normalizedEnvironment(_ environment: String) -> String {
        BrowserSyncCredentialsProvider.normalizedEnvironment(environment)
    }

    private func normalizedBaseUrl(_ baseUrl: String, environment: String) -> String {
        let normalized = baseUrl.trimmingCharacters(in: .whitespacesAndNewlines)
        return normalized.isEmpty ? BrowserSyncCredentialsProvider.defaultBaseUrl(for: environment) : normalized
    }
}
