import SwiftUI
import ActroVault

// MARK: - Password Strength

enum PasswordStrength {
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
        switch vaultPasswordStrength(password: password).level {
        case .weak: return .weak
        case .fair: return .fair
        case .good: return .good
        case .strong: return .strong
        }
    }
}

// MARK: - Reusable Password Field

struct PasswordField: View {
    @Binding var password: String
    var placeholder: String = "Master password"
    var showStrength: Bool = false

    @Binding var confirmPassword: String
    var showConfirm: Bool = false
    var confirmPlaceholder: String = "Confirm password"

    var hintTooltip: String?
    var onSubmit: (() -> Void)?

    private var passwordStrength: PasswordStrength {
        PasswordStrength.evaluate(password)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            SecureField(placeholder, text: $password)
                .textFieldStyle(.roundedBorder)
                .onSubmit { onSubmit?() }
                .overlay(alignment: .trailing) {
                    if let tip = hintTooltip, !tip.isEmpty {
                        Image(systemName: "info.circle")
                            .foregroundColor(.secondary)
                            .help("Hint: \(tip)")
                            .padding(.trailing, 6)
                    }
                }

            if showStrength && !password.isEmpty {
                strengthIndicator
            }

            if showConfirm {
                SecureField(confirmPlaceholder, text: $confirmPassword)
                    .textFieldStyle(.roundedBorder)
                    .onSubmit { onSubmit?() }

                if !confirmPassword.isEmpty && password != confirmPassword {
                    Text("Passwords do not match")
                        .font(.caption)
                        .foregroundColor(.red)
                }
            }

            if password.count > 0 && password.count < 5 {
                Text("Minimum 5 characters required")
                    .font(.caption)
                    .foregroundColor(.secondary)
            }
        }
    }

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
}

// Convenience initializer for non-confirm mode
extension PasswordField {
    init(
        password: Binding<String>,
        placeholder: String = "Master password",
        showStrength: Bool = false,
        hintTooltip: String? = nil,
        onSubmit: (() -> Void)? = nil
    ) {
        self._password = password
        self.placeholder = placeholder
        self.showStrength = showStrength
        self._confirmPassword = .constant("")
        self.showConfirm = false
        self.hintTooltip = hintTooltip
        self.onSubmit = onSubmit
    }
}
