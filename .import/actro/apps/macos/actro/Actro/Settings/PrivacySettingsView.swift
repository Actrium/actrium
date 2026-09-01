import SwiftUI
import WebKit

// MARK: - Time Range

enum ClearDataTimeRange: String, CaseIterable {
    case lastHour = "Last Hour"
    case lastDay = "Last 24 Hours"
    case allTime = "All Time"

    var date: Date {
        switch self {
        case .lastHour: return Date(timeIntervalSinceNow: -3600)
        case .lastDay: return Date(timeIntervalSinceNow: -86400)
        case .allTime: return Date.distantPast
        }
    }
}

// MARK: - Privacy Settings View

struct PrivacySettingsView: View {
    @State private var adBlockingEnabled = ContentBlockerManager.shared.isEnabled
    @State private var clearCookies = true
    @State private var clearCache = true
    @State private var clearLocalStorage = false
    @State private var clearIndexedDB = false
    @State private var timeRange: ClearDataTimeRange = .allTime
    @State private var isClearing = false
    @State private var showConfirmation = false
    @State private var clearResult: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            // Ad Blocking
            Text("Content Blocking")
                .font(.headline)

            Toggle("Block Ads and Trackers", isOn: $adBlockingEnabled)
                .onChange(of: adBlockingEnabled) { _, newValue in
                    ContentBlockerManager.shared.isEnabled = newValue
                }

            Text("Takes effect for newly opened tabs.")
                .font(.caption)
                .foregroundColor(.secondary)

            Divider()

            Text("Clear Browsing Data")
                .font(.headline)

            // Time range picker
            HStack {
                Text("Time Range:")
                Picker("", selection: $timeRange) {
                    ForEach(ClearDataTimeRange.allCases, id: \.self) { range in
                        Text(range.rawValue).tag(range)
                    }
                }
                .labelsHidden()
                .frame(width: 160)
            }

            // Data type checkboxes
            VStack(alignment: .leading, spacing: 8) {
                Toggle("Cookies", isOn: $clearCookies)
                Toggle("Cached Files", isOn: $clearCache)
                Toggle("Local Storage", isOn: $clearLocalStorage)
                Toggle("IndexedDB", isOn: $clearIndexedDB)
            }
            .toggleStyle(.checkbox)

            Divider()

            HStack {
                if let result = clearResult {
                    Text(result)
                        .font(.caption)
                        .foregroundColor(.secondary)
                }
                Spacer()
                Button("Clear Data") {
                    showConfirmation = true
                }
                .disabled(!hasSelection || isClearing)
                .alert("Clear Browsing Data?", isPresented: $showConfirmation) {
                    Button("Cancel", role: .cancel) {}
                    Button("Clear", role: .destructive) {
                        clearData()
                    }
                } message: {
                    Text("This will remove the selected data for \(timeRange.rawValue.lowercased()). This action cannot be undone.")
                }
            }

            Spacer()
        }
        .padding(20)
    }

    private var hasSelection: Bool {
        clearCookies || clearCache || clearLocalStorage || clearIndexedDB
    }

    private var selectedDataTypes: Set<String> {
        var types = Set<String>()
        if clearCookies { types.insert(WKWebsiteDataTypeCookies) }
        if clearCache {
            types.insert(WKWebsiteDataTypeDiskCache)
            types.insert(WKWebsiteDataTypeMemoryCache)
        }
        if clearLocalStorage { types.insert(WKWebsiteDataTypeLocalStorage) }
        if clearIndexedDB { types.insert(WKWebsiteDataTypeIndexedDBDatabases) }
        return types
    }

    private func clearData() {
        isClearing = true
        clearResult = nil
        let types = selectedDataTypes
        let since = timeRange.date

        WKWebsiteDataStore.default().removeData(
            ofTypes: types,
            modifiedSince: since
        ) {
            DispatchQueue.main.async {
                isClearing = false
                clearResult = "Data cleared successfully."
            }
        }
    }
}

#Preview {
    PrivacySettingsView()
        .frame(width: 450, height: 350)
}
