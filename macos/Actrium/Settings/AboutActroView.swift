import AppKit
import SwiftUI

/// The built-in `actro://about/` page.
///
/// This intentionally lives in the normal tab content hierarchy instead of an
/// `NSPanel`, so About follows the same focus, overview and window lifecycle as
/// every other Actro page.
struct AboutActroView: View {
    let isCompactMode: Bool

    init(isCompactMode: Bool = false) {
        self.isCompactMode = isCompactMode
    }

    var body: some View {
        VStack(spacing: isCompactMode ? 10 : 18) {
            Image(nsImage: NSApplication.shared.applicationIconImage)
                .resizable()
                .interpolation(.high)
                .frame(
                    width: isCompactMode ? 64 : 112,
                    height: isCompactMode ? 64 : 112
                )

            Text(applicationName)
                .font(isCompactMode ? .title2.weight(.semibold) : .largeTitle.weight(.semibold))

            Text(versionDescription)
                .font(isCompactMode ? .caption : .body)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(nsColor: .windowBackgroundColor))
    }

    private var applicationName: String {
        (Bundle.main.object(forInfoDictionaryKey: "CFBundleDisplayName") as? String)
            ?? (Bundle.main.object(forInfoDictionaryKey: "CFBundleName") as? String)
            ?? "Actro"
    }

    private var versionDescription: String {
        let version = (Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String) ?? "—"
        let build = (Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String) ?? "—"
        return "Version \(version) (\(build))"
    }
}

#Preview {
    AboutActroView()
        .frame(width: 900, height: 600)
}
