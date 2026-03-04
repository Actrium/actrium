import SwiftUI

struct UnlockVaultView: View {
    @ObservedObject var vault = VaultManager.shared

    @State private var password = ""
    @State private var errorMessage: String?

    private var hasTouchID: Bool {
        vault.isTouchIDEnabled && vault.isBiometricsAvailable
    }

    var body: some View {
        VStack(spacing: 0) {
            Spacer()

            VStack(spacing: 20) {
                Image(systemName: "lock.fill")
                    .font(.system(size: 36))
                    .foregroundColor(.secondary)

                Text("Vault Locked")
                    .font(.headline)

                Text("Enter your master password to unlock.")
                    .font(.subheadline)
                    .foregroundColor(.secondary)

                VStack(spacing: 12) {
                    PasswordField(
                        password: $password,
                        placeholder: "Master password",
                        onSubmit: { unlock() }
                    )

                    if let hint = vault.passwordHint, !hint.isEmpty {
                        Text("Hint: \(hint)")
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }

                    if let error = errorMessage {
                        Text(error)
                            .font(.caption)
                            .foregroundColor(.red)
                    }

                    HStack(spacing: 12) {
                        Button {
                            unlock()
                        } label: {
                            if vault.isProcessing {
                                HStack(spacing: 6) {
                                    ProgressView()
                                        .controlSize(.small)
                                    Text("Unlocking...")
                                }
                            } else {
                                Text("Unlock")
                            }
                        }
                        .buttonStyle(.borderedProminent)
                        .disabled(password.isEmpty || vault.isProcessing)
                        .opacity(!password.isEmpty && !vault.isProcessing ? 1 : 0.5)

                        if hasTouchID {
                            Button {
                                unlockWithBiometrics()
                            } label: {
                                Image(systemName: "touchid")
                                    .font(.system(size: 18))
                            }
                            .buttonStyle(.bordered)
                            .disabled(vault.isProcessing)
                        }
                    }
                }
                .frame(width: 280)
            }

            Spacer()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
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
            } catch {
                // Silent failure for auto-unlock — user can enter password manually
            }
        }
    }
}
