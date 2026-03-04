import SwiftUI

struct SyncSettingsView: View {
    var body: some View {
        VStack {
            Image(systemName: "arrow.triangle.2.circlepath")
                .font(.system(size: 40))
                .foregroundColor(.secondary)
            Text("Sync")
                .font(.headline)
            Text("Coming soon")
                .foregroundColor(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
