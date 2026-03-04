import SwiftUI

struct AutofillSuggestionItem: Identifiable {
    let id = UUID()
    let username: String
    let domain: String
    let password: String
}

struct AutofillSuggestionView: View {
    let items: [AutofillSuggestionItem]
    let onSelect: (AutofillSuggestionItem) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(items.enumerated()), id: \.element.id) { index, item in
                if index > 0 {
                    Divider()
                        .padding(.horizontal, 8)
                }
                AutofillSuggestionRow(item: item) {
                    onSelect(item)
                }
            }
        }
        .padding(.vertical, 4)
        .frame(width: 260)
    }
}

// MARK: - Vault Locked Prompt

struct VaultLockedPromptView: View {
    let onUnlock: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "lock.fill")
                .font(.system(size: 14))
                .foregroundColor(.secondary)
                .frame(width: 20)

            Text("Vault is locked")
                .font(.system(size: 13))
                .foregroundColor(.primary)

            Spacer()

            Button("Unlock") {
                onUnlock()
            }
            .controlSize(.small)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .frame(width: 260)
    }
}

// MARK: - Suggestion Row

private struct AutofillSuggestionRow: View {
    let item: AutofillSuggestionItem
    let action: () -> Void
    @State private var isHovered = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                Image(systemName: "key.fill")
                    .font(.system(size: 14))
                    .foregroundColor(.secondary)
                    .frame(width: 20)

                VStack(alignment: .leading, spacing: 1) {
                    Text(item.username.isEmpty ? "(no username)" : item.username)
                        .font(.system(size: 13))
                        .foregroundColor(.primary)
                        .lineLimit(1)
                    Text(item.domain)
                        .font(.system(size: 11))
                        .foregroundColor(.secondary)
                        .lineLimit(1)
                }

                Spacer()
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .background(isHovered ? Color.accentColor.opacity(0.1) : Color.clear)
        .cornerRadius(4)
        .padding(.horizontal, 4)
        .onHover { hovering in
            isHovered = hovering
        }
    }
}
