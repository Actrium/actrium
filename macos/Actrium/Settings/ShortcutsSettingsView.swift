import SwiftUI

struct ShortcutsSettingsView: View {
    @StateObject private var store = ShortcutStore.shared

    @State private var conflictAlert: ConflictInfo?

    private struct ConflictInfo: Identifiable {
        let id = UUID()
        let action: ShortcutAction
        let conflictsWith: ShortcutAction
        let key: String
        let modifiers: NSEvent.ModifierFlags
    }

    var body: some View {
        VStack(spacing: 0) {
            List {
                ForEach(ShortcutCategory.allCases, id: \.self) { category in
                    Section(header: Text(category.rawValue)) {
                        ForEach(actionsForCategory(category), id: \.self) { action in
                            shortcutRow(for: action)
                        }
                    }
                }
            }
            .listStyle(.inset)

            Divider()

            HStack {
                Spacer()
                Button("Reset All to Defaults") {
                    store.resetAllToDefaults()
                }
                .padding(12)
            }
        }
        .alert(item: $conflictAlert) { info in
            Alert(
                title: Text("Shortcut Conflict"),
                message: Text("The shortcut \(ShortcutBinding(key: info.key, modifiers: info.modifiers.rawValue).displayString) is already used by \"\(info.conflictsWith.displayName)\". Please choose a different shortcut."),
                dismissButton: .default(Text("OK"))
            )
        }
    }

    // MARK: - Helpers

    private func actionsForCategory(_ category: ShortcutCategory) -> [ShortcutAction] {
        ShortcutAction.allCases.filter { $0.category == category }
    }

    private func shortcutRow(for action: ShortcutAction) -> some View {
        HStack {
            Text(action.displayName)
                .frame(maxWidth: .infinity, alignment: .leading)

            ShortcutRecorderView(
                currentBinding: store.bindings[action],
                onRecord: { key, modifiers in
                    handleRecord(action: action, key: key, modifiers: modifiers)
                }
            )
            .frame(width: 120, height: 24)

            Button(action: {
                store.resetBinding(for: action)
            }) {
                Image(systemName: "arrow.counterclockwise")
                    .foregroundColor(.secondary)
            }
            .buttonStyle(.plain)
            .help("Reset to default")
            .opacity(store.bindings[action] != store.defaultBinding(for: action) ? 1 : 0.3)
        }
        .padding(.vertical, 2)
    }

    private func handleRecord(action: ShortcutAction, key: String, modifiers: NSEvent.ModifierFlags) {
        // Empty key means clear
        if key.isEmpty {
            store.resetBinding(for: action)
            return
        }

        if let conflict = store.hasConflict(key: key, modifiers: modifiers, excluding: action) {
            conflictAlert = ConflictInfo(
                action: action,
                conflictsWith: conflict,
                key: key,
                modifiers: modifiers
            )
            return
        }

        store.updateBinding(for: action, key: key, modifiers: modifiers)
    }
}
