import SwiftUI

struct SetupMasterPasswordView: View {
    @ObservedObject var vault = VaultManager.shared

    @State private var password = ""
    @State private var confirmPassword = ""
    @State private var enableTouchID = false
    @State private var errorMessage: String?

    private var passwordStrength: PasswordStrength {
        PasswordStrength.evaluate(password)
    }

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

                VStack(alignment: .leading, spacing: 12) {
                    SecureField("Master password", text: $password)
                        .textFieldStyle(.roundedBorder)

                    if !password.isEmpty {
                        strengthIndicator
                    }

                    SecureField("Confirm password", text: $confirmPassword)
                        .textFieldStyle(.roundedBorder)

                    if !confirmPassword.isEmpty && password != confirmPassword {
                        Text("Passwords do not match")
                            .font(.caption)
                            .foregroundColor(.red)
                    }

                    if password.count > 0 && password.count < 5 {
                        Text("Minimum 5 characters required")
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }
                }
                .frame(width: 280)

                if vault.isBiometricsAvailable {
                    Toggle("Enable Touch ID", isOn: $enableTouchID)
                        .frame(width: 280)
                }

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
        .onAppear {
            enableTouchID = vault.isBiometricsAvailable
        }
    }

    // MARK: - Strength Indicator

    private var strengthIndicator: some View {
        HStack(spacing: 4) {
            ForEach(0..<4) { index in
                RoundedRectangle(cornerRadius: 2)
                    .fill(index < passwordStrength.level ? passwordStrength.color : Color.gray.opacity(0.3))
                    .frame(height: 4)
            }
            Text(passwordStrength.label)
                .font(.caption2)
                .foregroundColor(passwordStrength.color)
                .frame(width: 44, alignment: .trailing)
        }
    }

    private func setup() {
        errorMessage = nil
        Task {
            do {
                try await vault.setupMasterPassword(password, enableTouchID: enableTouchID)
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }
}

// MARK: - Password Strength

private enum PasswordStrength {
    case weak, fair, good, strong

    var level: Int {
        switch self {
        case .weak: return 1
        case .fair: return 2
        case .good: return 3
        case .strong: return 4
        }
    }

    var label: String {
        switch self {
        case .weak: return "Weak"
        case .fair: return "Fair"
        case .good: return "Good"
        case .strong: return "Strong"
        }
    }

    var color: Color {
        switch self {
        case .weak: return .red
        case .fair: return .orange
        case .good: return .yellow
        case .strong: return .green
        }
    }

    static func evaluate(_ password: String) -> PasswordStrength {
        var score = 0
        if password.count >= 5 { score += 1 }
        if password.count >= 10 { score += 1 }
        if password.rangeOfCharacter(from: .uppercaseLetters) != nil &&
           password.rangeOfCharacter(from: .lowercaseLetters) != nil { score += 1 }
        if password.rangeOfCharacter(from: .decimalDigits) != nil { score += 1 }
        if password.rangeOfCharacter(from: .punctuationCharacters) != nil ||
           password.rangeOfCharacter(from: .symbols) != nil { score += 1 }

        switch score {
        case 0...1: return .weak
        case 2: return .fair
        case 3: return .good
        default: return .strong
        }
    }
}
