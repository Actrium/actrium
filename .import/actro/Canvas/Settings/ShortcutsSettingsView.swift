import SwiftUI

struct ShortcutsSettingsView: View {
    var body: some View {
        VStack {
            Image(systemName: "keyboard")
                .font(.system(size: 40))
                .foregroundColor(.secondary)
            Text("Shortcuts")
                .font(.headline)
            Text("Coming soon")
                .foregroundColor(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
