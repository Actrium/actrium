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

// MARK: - Vault Locked Prompt (inline unlock)

struct VaultLockedPromptView: View {
    @ObservedObject private var vault = VaultManager.shared
    let onUnlocked: () -> Void

    @State private var password = ""
    @State private var errorMessage: String?

    private var hasTouchID: Bool {
        vault.isTouchIDEnabled && vault.isBiometricsAvailable
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Image(systemName: "lock.fill")
                    .font(.system(size: 14))
                    .foregroundColor(.secondary)
                    .frame(width: 20)
                Text("Vault is locked")
                    .font(.system(size: 13, weight: .medium))
                    .foregroundColor(.primary)
            }

            HStack(spacing: 6) {
                SecureField("Master password", text: $password)
                    .textFieldStyle(.roundedBorder)
                    .font(.system(size: 12))
                    .onSubmit { unlock() }

                Button {
                    unlock()
                } label: {
                    if vault.isProcessing {
                        ProgressView()
                            .controlSize(.small)
                    } else {
                        Text("Unlock")
                    }
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.small)
                .disabled(password.isEmpty || vault.isProcessing)

                if hasTouchID {
                    Button {
                        unlockWithBiometrics()
                    } label: {
                        Image(systemName: "touchid")
                            .font(.system(size: 14))
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    .disabled(vault.isProcessing)
                }
            }

            if let error = errorMessage {
                Text(error)
                    .font(.system(size: 11))
                    .foregroundColor(.red)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .frame(width: 280)
        .onAppear {
            if hasTouchID {
                unlockWithBiometrics()
            }
        }
    }

    private func unlock() {
        errorMessage = nil
        Task {
            do {
                try await vault.unlock(password: password)
                onUnlocked()
            } catch {
                errorMessage = "Incorrect password"
            }
        }
    }

    private func unlockWithBiometrics() {
        errorMessage = nil
        Task {
            do {
                try await vault.unlockWithBiometrics()
                onUnlocked()
            } catch {
                // Silent — user can enter password manually
            }
        }
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
