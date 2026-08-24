import SwiftUI

// MARK: - Settings Tab

enum SettingsTab: String, CaseIterable {
    case general = "General"
    case appearance = "Appearance"
    case password = "Password"
    case privacy = "Privacy"
    case sync = "Sync"
    case shortcuts = "Shortcuts"

    var icon: String {
        switch self {
        case .general: return "gearshape"
        case .appearance: return "circle.lefthalf.filled"
        case .password: return "lock.fill"
        case .privacy: return "hand.raised.fill"
        case .sync: return "arrow.triangle.2.circlepath"
        case .shortcuts: return "keyboard"
        }
    }

    var url: URL {
        switch self {
        case .general:
            return SystemTabType.settings.systemURL
        default:
            return URL(string: "actro://settings/\(rawValue.lowercased())")!
        }
    }

    static func from(url: URL?) -> SettingsTab {
        guard let url,
              url.scheme?.lowercased() == ActroAppURLScheme.primary,
              url.host?.lowercased() == "settings" else {
            return .general
        }

        let component = url.pathComponents
            .first(where: { $0 != "/" })?
            .lowercased()
        guard let component else { return .general }
        return allCases.first(where: { $0.rawValue.lowercased() == component }) ?? .general
    }
}

// MARK: - Settings View

struct SettingsView: View {
    let isCompactMode: Bool
    let initialTab: SettingsTab
    let onSelectionChange: (SettingsTab) -> Void

    @State private var selectedTab: SettingsTab

    init(
        isCompactMode: Bool = false,
        initialTab: SettingsTab = .general,
        onSelectionChange: @escaping (SettingsTab) -> Void = { _ in }
    ) {
        self.isCompactMode = isCompactMode
        self.initialTab = initialTab
        self.onSelectionChange = onSelectionChange
        _selectedTab = State(initialValue: initialTab)
    }

    @ViewBuilder
    var body: some View {
        if isCompactMode {
            compactContent
        } else {
            VStack(spacing: 0) {
                // Toolbar-style tab bar
                tabBar

                Divider()

                // Content
                tabContent
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .onChange(of: initialTab) { _, newTab in
                selectedTab = newTab
            }
        }
    }

    private var compactContent: some View {
        VStack(spacing: 14) {
            Image(systemName: "gearshape")
                .font(.system(size: 38, weight: .medium))
            Text("Settings")
                .font(.title2.weight(.semibold))
        }
        .foregroundStyle(.secondary)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(nsColor: .windowBackgroundColor))
    }

    // MARK: - Tab Bar

    private var tabBar: some View {
        HStack(spacing: 24) {
            ForEach(SettingsTab.allCases, id: \.self) { tab in
                tabButton(for: tab)
            }
        }
        .padding(.vertical, 12)
        .frame(maxWidth: .infinity)
        .background(Color(nsColor: .windowBackgroundColor))
    }

    private func tabButton(for tab: SettingsTab) -> some View {
        Button(action: {
            selectedTab = tab
            onSelectionChange(tab)
        }) {
            VStack(spacing: 4) {
                Image(systemName: tab.icon)
                    .font(.system(size: 20))
                Text(tab.rawValue)
                    .font(.system(size: 11))
            }
            .foregroundColor(selectedTab == tab ? .accentColor : .secondary)
            .frame(width: 70, height: 44)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    // MARK: - Tab Content

    @ViewBuilder
    private var tabContent: some View {
        switch selectedTab {
        case .general:
            GeneralSettingsView()
        case .appearance:
            AppearanceSettingsView()
        case .password:
            PasswordSettingsView()
        case .privacy:
            PrivacySettingsView()
        case .sync:
            SyncSettingsView()
        case .shortcuts:
            ShortcutsSettingsView()
        }
    }
}

#Preview {
    SettingsView()
}
