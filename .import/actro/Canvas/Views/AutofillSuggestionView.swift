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
    var onManagePasswords: (() -> Void)?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Credential rows
            ForEach(items) { item in
                AutofillSuggestionRow(item: item) {
                    onSelect(item)
                }
            }

            // Separator + Manage Passwords
            Rectangle()
                .fill(Color.primary.opacity(0.12))
                .frame(height: 1)
                .padding(.vertical, 2)

            ManagePasswordsRow {
                onManagePasswords?()
            }
        }
        .padding(.vertical, 4)
        .frame(width: 280)
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
        VStack(alignment: .leading, spacing: 12) {
            // Header
            HStack(spacing: 10) {
                ZStack {
                    Circle()
                        .fill(Color.primary.opacity(0.08))
                        .frame(width: 32, height: 32)
                    Image(systemName: "lock.fill")
                        .font(.system(size: 14))
                        .foregroundColor(.secondary)
                }
                VStack(alignment: .leading, spacing: 2) {
                    Text("Vault is locked")
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundColor(.primary)
                    Text("Enter master password to autofill")
                        .font(.system(size: 11))
                        .foregroundColor(.secondary)
                }
            }

            // Password input row
            HStack(spacing: 8) {
                SecureField("Master password", text: $password)
                    .textFieldStyle(.roundedBorder)
                    .font(.system(size: 13))
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
                .controlSize(.regular)
                .disabled(password.isEmpty || vault.isProcessing)
            }

            if hasTouchID {
                Text("Or use Touch ID when prompted")
                    .font(.system(size: 11))
                    .foregroundColor(.secondary)
            }

            if let error = errorMessage {
                Text(error)
                    .font(.system(size: 11))
                    .foregroundColor(.red)
            }
        }
        .padding(16)
        .frame(width: 320)
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
                NSLog("🔑 VaultLockedPrompt: unlock succeeded, calling onUnlocked")
                onUnlocked()
            } catch {
                NSLog("🔑 VaultLockedPrompt: unlock failed - %@", "\(error)")
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

// MARK: - Credential Row (Chrome style)

private struct AutofillSuggestionRow: View {
    let item: AutofillSuggestionItem
    let action: () -> Void
    @State private var isHovered = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 10) {
                // Circular key icon (Chrome-style)
                ZStack {
                    Circle()
                        .fill(Color.primary.opacity(0.08))
                        .frame(width: 28, height: 28)
                    Image(systemName: "key.fill")
                        .font(.system(size: 12))
                        .foregroundColor(.secondary)
                        .rotationEffect(.degrees(-45))
                }

                VStack(alignment: .leading, spacing: 2) {
                    Text(item.username.isEmpty ? "(no username)" : item.username)
                        .font(.system(size: 13))
                        .foregroundColor(.primary)
                        .lineLimit(1)
                    Text("••••••••")
                        .font(.system(size: 11))
                        .foregroundColor(.secondary)
                        .lineLimit(1)
                }

                Spacer()
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .background(isHovered ? Color.primary.opacity(0.08) : Color.clear)
        .onHover { hovering in
            isHovered = hovering
        }
    }
}

// MARK: - Manage Passwords Row

private struct ManagePasswordsRow: View {
    let action: () -> Void
    @State private var isHovered = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 10) {
                ZStack {
                    Circle()
                        .fill(Color.primary.opacity(0.08))
                        .frame(width: 28, height: 28)
                    Image(systemName: "gearshape.fill")
                        .font(.system(size: 12))
                        .foregroundColor(.secondary)
                }

                Text("Manage Passwords…")
                    .font(.system(size: 13))
                    .foregroundColor(.primary)

                Spacer()

                // Colored key icon (brand accent, like Chrome's)
                Image(systemName: "key.fill")
                    .font(.system(size: 14))
                    .foregroundStyle(
                        LinearGradient(
                            colors: [.green, .blue, .yellow],
                            startPoint: .topLeading,
                            endPoint: .bottomTrailing
                        )
                    )
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .background(isHovered ? Color.primary.opacity(0.08) : Color.clear)
        .onHover { hovering in
            isHovered = hovering
        }
    }
}
