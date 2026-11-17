import SwiftUI

struct GeneralSettingsView: View {
    @State private var selectedEngine: SearchEngine = .current
    @State private var selectedAppearance = AppearanceManager.shared.mode
    @State private var selectedWindowChrome = WindowChromeManager.shared.style

    var body: some View {
        Form {
            Section("Appearance") {
                Picker("Mode", selection: $selectedAppearance) {
                    ForEach(AppAppearanceMode.allCases) { mode in
                        Text(mode.title).tag(mode)
                    }
                }
                .pickerStyle(.segmented)
                .onChange(of: selectedAppearance) { _, mode in
                    AppearanceManager.shared.select(mode)
                }

                Picker("Frame and Header", selection: $selectedWindowChrome) {
                    ForEach(WindowChromeStyle.allCases) { style in
                        Text(style.title).tag(style)
                    }
                }
                .onChange(of: selectedWindowChrome) { _, style in
                    WindowChromeManager.shared.select(style)
                }
            }
            Section("Search") {
                Picker("Default Search Engine", selection: $selectedEngine) {
                    ForEach(SearchEngine.allCases) { engine in
                        Label(engine.displayName, systemImage: engine.icon)
                            .tag(engine)
                    }
                }
                .onChange(of: selectedEngine) { _, newValue in
                    SearchEngine.current = newValue
                }
            }
        }
        .formStyle(.grouped)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    }
}

#Preview {
    GeneralSettingsView()
}
