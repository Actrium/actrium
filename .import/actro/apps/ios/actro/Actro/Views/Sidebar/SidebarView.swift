import SwiftUI

struct SidebarView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    @ObservedObject var windowState: WindowStateManager = .shared

    var body: some View {
        VStack(spacing: 0) {
            // Tab count header
            HStack {
                Text("\(viewModel.tabs.count) Tab\(viewModel.tabs.count == 1 ? "" : "s")")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundColor(.secondary)
                    .textCase(.uppercase)
                Spacer()
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)

            Spacer(minLength: 0)
        }
    }
}

#Preview {
    SidebarView(viewModel: BrowserViewModel(), historyVM: HistoryViewModel())
}
