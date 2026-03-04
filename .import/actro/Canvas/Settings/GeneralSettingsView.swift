import SwiftUI

struct GeneralSettingsView: View {
    @State private var selectedEngine: SearchEngine = .current

    var body: some View {
        Form {
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
