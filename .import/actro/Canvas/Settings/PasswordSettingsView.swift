import SwiftUI

struct PasswordSettingsView: View {
    var body: some View {
        VStack {
            Image(systemName: "lock.fill")
                .font(.system(size: 40))
                .foregroundColor(.secondary)
            Text("Password Vault")
                .font(.headline)
            Text("Coming soon")
                .foregroundColor(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
