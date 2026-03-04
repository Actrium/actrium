import SwiftUI

// MARK: - Settings Tab

enum SettingsTab: String, CaseIterable {
    case password = "Password"
    case sync = "Sync"
    case shortcuts = "Shortcuts"

    var icon: String {
        switch self {
        case .password: return "lock.fill"
        case .sync: return "arrow.triangle.2.circlepath"
        case .shortcuts: return "keyboard"
        }
    }
}

// MARK: - Settings View

struct SettingsView: View {
    @State private var selectedTab: SettingsTab = .password

    var body: some View {
        VStack(spacing: 0) {
            // Toolbar-style tab bar
            tabBar

            Divider()

            // Content
            tabContent
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .frame(width: 550, height: 420)
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
        Button(action: { selectedTab = tab }) {
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
        case .password:
            PasswordSettingsView()
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
