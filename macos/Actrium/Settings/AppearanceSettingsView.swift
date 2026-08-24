import SwiftUI

struct AppearanceSettingsView: View {
    @State private var selectedMode = AppearanceManager.shared.mode
    @State private var selectedWindowChrome = WindowChromeManager.shared.style

    var body: some View {
        Form {
            Section("Appearance") {
                Picker("Mode", selection: $selectedMode) {
                    ForEach(AppAppearanceMode.allCases) { mode in
                        Text(mode.title).tag(mode)
                    }
                }
                .pickerStyle(.segmented)
                .onChange(of: selectedMode) { _, newMode in
                    AppearanceManager.shared.select(newMode)
                }

                Text("跟随系统会自动匹配 macOS 的浅色或深色外观。")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Section("Window color") {
                Picker("Frame and Header", selection: $selectedWindowChrome) {
                    ForEach(WindowChromeStyle.allCases) { style in
                        Text(style.title).tag(style)
                    }
                }
                .onChange(of: selectedWindowChrome) { _, newStyle in
                    WindowChromeManager.shared.select(newStyle)
                }

                Text("外框与 Header 使用同一套半透明玻璃材质。")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    }
}

#Preview {
    AppearanceSettingsView()
        .frame(width: 720, height: 520)
}
