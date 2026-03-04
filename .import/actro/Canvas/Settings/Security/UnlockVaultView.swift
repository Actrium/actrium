import SwiftUI

struct UnlockVaultView: View {
    @ObservedObject var vault = VaultManager.shared

    @State private var password = ""
    @State private var errorMessage: String?
    @State private var isUnlocking = false

    private var hasTouchID: Bool {
        vault.isTouchIDEnabled && KeychainHelper.isBiometricsAvailable()
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
                    SecureField("Master password", text: $password)
                        .textFieldStyle(.roundedBorder)
                        .onSubmit { unlock() }

                    if let error = errorMessage {
                        Text(error)
                            .font(.caption)
                            .foregroundColor(.red)
                    }

                    HStack(spacing: 12) {
                        Button("Unlock") {
                            unlock()
                        }
                        .buttonStyle(.borderedProminent)
                        .disabled(password.isEmpty || isUnlocking)

                        if hasTouchID {
                            Button {
                                unlockWithBiometrics()
                            } label: {
                                Image(systemName: "touchid")
                                    .font(.system(size: 18))
                            }
                            .buttonStyle(.bordered)
                            .disabled(isUnlocking)
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
        isUnlocking = true
        do {
            try vault.unlock(password: password)
        } catch {
            errorMessage = "Incorrect password"
        }
        isUnlocking = false
    }

    private func unlockWithBiometrics() {
        errorMessage = nil
        isUnlocking = true
        Task {
            do {
                try await vault.unlockWithBiometrics()
            } catch {
                errorMessage = "Biometric authentication failed"
            }
            isUnlocking = false
        }
    }
}
