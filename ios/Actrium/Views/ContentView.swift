import SwiftUI
import UniformTypeIdentifiers

struct ContentView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    var body: some View {
        Group {
            if UIDevice.current.userInterfaceIdiom == .phone {
                PhoneMainView(viewModel: viewModel, historyVM: historyVM)
            } else {
                MainWindowView(viewModel: viewModel, historyVM: historyVM)
            }
        }
    }
}

struct SearchBar: View {
    @ObservedObject var viewModel: BrowserViewModel
    
    var body: some View {
        VStack {
            Spacer()
            
            HStack {
                Spacer()
                
                HStack(spacing: 12) {
                    Image(systemName: "magnifyingglass")
                        .foregroundColor(Color(hex: "#999"))
                        .font(.system(size: 18))
                    
                    Text("\(TM.active.search())...")
                        .foregroundColor(Color(hex: "#999"))
                    
                    Spacer()
                }
                .padding(.horizontal, 11)
                .frame(maxWidth: 500)
                .frame(height: 48)
                .background(Color.white)
                .cornerRadius(24)
                .onTapGesture {
                    viewModel.previousSystemTabType = viewModel.currentSystemTabType
                    viewModel.openSystemTab(.globalSearch, additionalProps: nil, replaceAdditionalProps: true)
                    viewModel.currentSystemTabType = .globalSearch
                }
                
                Spacer()
            }
            .padding(.bottom, 18)
        }
    }
}

#Preview {
    ContentView(viewModel: BrowserViewModel(), historyVM: HistoryViewModel())
}
