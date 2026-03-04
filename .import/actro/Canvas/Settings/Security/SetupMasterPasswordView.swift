import SwiftUI

struct SetupMasterPasswordView: View {
    @ObservedObject var vault = VaultManager.shared

    @State private var password = ""
    @State private var confirmPassword = ""
    @State private var hint = ""
    @State private var errorMessage: String?

    private var isValid: Bool {
        password.count >= 5 && password == confirmPassword
    }

    var body: some View {
        VStack(spacing: 0) {
            Spacer()

            VStack(spacing: 20) {
                Image(systemName: "lock.shield")
                    .font(.system(size: 36))
                    .foregroundColor(.accentColor)

                Text("Set Up Password Vault")
                    .font(.headline)

                Text("Create a master password to protect your saved credentials.")
                    .font(.subheadline)
                    .foregroundColor(.secondary)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 320)

                PasswordField(
                    password: $password,
                    placeholder: "Master password",
                    showStrength: true,
                    confirmPassword: $confirmPassword,
                    showConfirm: true
                )
                .frame(width: 280)

                TextField("Password hint (optional)", text: $hint)
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 280)

                if let error = errorMessage {
                    Text(error)
                        .font(.caption)
                        .foregroundColor(.red)
                }

                Button {
                    setup()
                } label: {
                    if vault.isProcessing {
                        HStack(spacing: 6) {
                            ProgressView()
                                .controlSize(.small)
                            Text("Setting up...")
                        }
                    } else {
                        Text("Set Password")
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(!isValid || vault.isProcessing)
                .opacity(isValid && !vault.isProcessing ? 1 : 0.5)
            }

            Spacer()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func setup() {
        errorMessage = nil
        Task {
            do {
                try await vault.setupMasterPassword(password, hint: hint.isEmpty ? nil : hint)
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }
}
