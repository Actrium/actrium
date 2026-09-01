import SwiftUI
import Kingfisher

struct PhoneTabListView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel
    var animation: Namespace.ID
    var onSelect: (Tab) -> Void
    
    var body: some View {
        ZStack(alignment: .top) {
            Color.black.ignoresSafeArea()
            
            PhoneNavigationBar(title: "Actro", showLargeTitle: true, rightItems: {
                ZStack {
                    Circle().fill(Color.accent)
                    Image(systemName: "plus")
                        .renderingMode(.template)
                        .font(.system(size: 19))
                        .frame(width: 44, height: 44)
                        .foregroundColor(.white)
                        .onTapGesture {
                            let newTab = viewModel.addNewTab(deferAppend: true)
                            onSelect(newTab)
                        }
                }
                .asGlassClearEffect(.accent)
                .asCornerRadius(22)
            })
            .ignoresSafeArea(edges: .top)
            .zIndex(10)
            
            if viewModel.tabs.isEmpty {
                // Empty list place: in ZStack Centre, no. ScrollView Impact
                VStack {
                    Spacer()
                    VStack(spacing: 43) {
                        Image("active_black")
                            .resizable()
                            .aspectRatio(contentMode: .fit)
                            .frame(height: 52)
                        
                        Text("\(TM.emptyActive.title())\n\(TM.emptyActive.subtitle())")
                            .font(.system(size: 20))
                            .foregroundColor(Color(hex: "#565656"))
                            .multilineTextAlignment(.center)
                            .lineSpacing(3)
                    }
                    Spacer()
                }
                .ignoresSafeArea()
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }

            ScrollViewReader { proxy in
                ScrollView {
                    // Top spacing: Adapt to custom navigation bar
                    Color.clear.frame(height: UIApplication.shared.statusBarHeight + 66 + 26)

                    if !viewModel.tabs.isEmpty {
                        LazyVStack(spacing: 16) {
                            ForEach(viewModel.tabs) { tab in
                                PhoneTabCardView(
                                    tab: tab,
                                    viewModel: viewModel,
                                    historyVM: historyVM,
                                    animation: animation,
                                    onSelect: {
                                        // Switch Now IDMake sure. UI Response (Blue Border)
                                        withAnimation(.easeInOut(duration: 0.2)) {
                                            viewModel.currentTabId = tab.id
                                        }
                                        // Delay a little bit to get to the full screen so that the user can see the selection.
                                        DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) {
                                            onSelect(tab)
                                        }
                                    },
                                    onClose: {
                                        withAnimation(.spring(response: 0.35, dampingFraction: 0.8)) {
                                            viewModel.closeTab(id: tab.id)
                                        }
                                    }
                                )
                                .id(tab.id)
                            }
                            Color.clear.frame(height: 80).id("bottomSpacer")
                        }
                        .padding(.horizontal, 16)
                    }
                }
                .ignoresSafeArea()
                .onChange(of: viewModel.tabs.count) { oldCount, newCount in
                    if newCount > oldCount {
                        withAnimation {
                            proxy.scrollTo("bottomSpacer", anchor: .bottom)
                        }
                    }
                }
            }
        }
        .background(Color.black)
    }
}

// Sub-component: Pagemark card, reference ASActiveItem Layout
struct PhoneTabCardView: View {
    let tab: Tab
    @ObservedObject var viewModel: BrowserViewModel
    var historyVM: HistoryViewModel
    var animation: Namespace.ID
    let onSelect: () -> Void
    let onClose: () -> Void
    
    @EnvironmentObject private var theme: ThemeManager

    private let iconWidth = 32.0

    private func toggleBookmark() {
        let newBookmarkStatus = !tab.isBookmarked
        if let index = viewModel.tabs.firstIndex(where: { $0.id == tab.id }) {
            viewModel.tabs[index].isBookmarked = newBookmarkStatus
            viewModel.scheduleSaveTabs()
        }
        NotificationCenter.default.post(
            name: .tabBookmarkChangedFromNative,
            object: nil,
            userInfo: tab.runtimeBookmarkChangedUserInfo(isBookmarked: newBookmarkStatus)
        )
    }

    private func closeOtherTabs() {
        withAnimation(.spring(response: 0.35, dampingFraction: 0.8)) {
            viewModel.closeOtherTabs(keepTabId: tab.id)
        }
    }
    
    var body: some View {
        ZStack(alignment: .bottom) {
            // Content Preview Area
            HStack {
                if let snapshot = viewModel.tabSnapshots[tab.id] {
                    GeometryReader { geo in
                        Image(uiImage: snapshot)
                            .resizable()
                            .aspectRatio(contentMode: .fill)
                            .frame(width: geo.size.width, height: geo.size.height - 56)
                    }
                } else {
                    Color.white
                }
            }
            .clipped()
            .allowsHitTesting(false)
            .contentShape(Rectangle())
            .onTapGesture {
                onSelect()
            }
            
            // Bottom service line
            HStack(alignment: .center, spacing: 12) {
                // Apply Icon
                ZStack(alignment: .topTrailing) {
                    Circle()
                        .fill(Color(hslString: tab.backgroundColor))
                        .frame(width: iconWidth, height: iconWidth)
                        .overlay {
                            if tab.status == .dead {
                                Image("service_stop")
                                    .font(.system(size: iconWidth))
                            } else if let icon = tab.icon, let url = URL(string: icon) {
                                KFImage(url)
                                    .resizable()
                                    .scaledToFit()
                                    .frame(width: iconWidth, height: iconWidth)
                                    .cornerRadius(iconWidth/2)
                            } else {
                                Image(systemName: "plus.viewfinder")
                                    .font(.system(size: 14))
                                    .foregroundColor(.white.opacity(0.9))
                            }
                        }
                    
                    // Unread message small red dot
                    if tab.hasUnreadMessage {
                        Circle()
                            .fill(Color.red)
                            .frame(width: 10, height: 10)
                            .overlay(
                                Circle()
                                    .stroke(Color(.systemBackground), lineWidth: 2)
                            )
                            .offset(x: 2, y: -2)
                    }
                }
                .frame(width: iconWidth, height: iconWidth)
                
                // Title
                Text(tab.status == .dead ? "\(tab.title)(\(TM.tabStatus.serverClosed()))" : tab.title)
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundColor(theme.isDarkMode ? Color(hex: "#ECECED") : Color(hex: "#404040"))
                    .lineLimit(1)
                
                Spacer()
                
                // Collection Star (titled "Step-In-Sight")
                if tab.title != TM.active.blackTab() {
                    Button(action: {
                        toggleBookmark()
                    }) {
                        Image(systemName: "star.circle.fill")
                            .font(.system(size: 20))
                            .foregroundColor(tab.isBookmarked ? Color(hex: "#EAC15B") : Color(hex: "#9E9EA7"))
                            .frame(width: 36, height: 36)
                            .background(Circle().fill(Color.clear))
                    }
                }
                
                // Close button
                Button(action: onClose) {
                    Image(systemName: "xmark.circle.fill")
                        .font(.system(size: 20))
                        .foregroundColor(Color(hex: "#9E9EA7"))
                        .frame(width: 36, height: 36)
                }
            }
            .frame(height: 56)
            .padding(.horizontal, 14)
            .background(Color(hex: theme.isDarkMode ? "#2C2C2C" : "#E5E5EA"))
        }
        .frame(maxWidth: .infinity)
        .aspectRatio(1 / 0.54, contentMode: .fit)
        .clipShape(RoundedRectangle(cornerRadius: 22))
        .shadow(color: Color.black.opacity(0.1), radius: 8, x: 0, y: 2)
        .asMatchedTransitionSource(id: tab.id, in: animation)
        .onAppear {
            // Memory Warning Recovered Snapshots When Cards appear, load from disk as necessary
            viewModel.loadSnapshotIfNeeded(for: tab.id)
        }
        .contextMenu {
            if tab.title != TM.active.blackTab() {
                Button {
                    toggleBookmark()
                } label: {
                    Label(tab.isBookmarked ? TM.tabContextMenu.unbookmark() : TM.tabContextMenu.bookmark(),
                          systemImage: tab.isBookmarked ? "star.slash" : "star")
                }
            }

            Button(role: .destructive) {
                onClose()
            } label: {
                Label(TM.tabContextMenu.closeTab(), systemImage: "xmark")
            }

            if viewModel.tabs.count > 1 {
                Divider()
                Button(role: .destructive) {
                    closeOtherTabs()
                } label: {
                    Label(TM.tabContextMenu.closeOtherTabs(), systemImage: "xmark")
                }
            }
        }
    }
}

struct PhoneBrowserView: View {
    let tab: Tab
    @ObservedObject var viewModel: BrowserViewModel
    var historyVM: HistoryViewModel

    var body: some View {
        GeometryReader { geometry in
            SingleTabRootView(
                tab: tab,
                viewModel: viewModel,
                historyVM: historyVM,
                containerSize: geometry.size
            )
            .id(tab.id)
            .onAppear {
                // Tab Enter Full Screen View
                NotificationCenter.default.post(
                    name: .tabEnteredFullscreenFromNative,
                    object: nil,
                    userInfo: tab.runtimeFullscreenEnteredUserInfo
                )
            }
            .onDisappear {
                // Tab Exit fullscreen view
                NotificationCenter.default.post(
                    name: .tabExitedFullscreenFromNative,
                    object: nil,
                    userInfo: tab.runtimeFullscreenExitedUserInfo
                )
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Color.black)
            .background(
                SheetWillDismissObserver(tabId: tab.id, viewModel: viewModel)
            )
        }
    }
}

// MARK: - Sheet The transects when the gestures are closed.

/// Pass. UIAdaptivePresentationControllerDelegate Yes. sheet Start of sign-off (willDismiss)
/// Synchronizes the snapshot, at which point the page remains fully visible.
private struct SheetWillDismissObserver: UIViewRepresentable {
    let tabId: UUID
    let viewModel: BrowserViewModel

    func makeUIView(context: Context) -> UIView {
        let view = ObserverView()
        view.delegate = context.coordinator
        // Delay to Next RunLoopMake sure. UIViewController Mounted to View Level
        DispatchQueue.main.async {
            guard let vc = view.findHostingViewController() else { return }
            vc.presentationController?.delegate = context.coordinator
        }
        return view
    }

    func updateUIView(_ uiView: UIView, context: Context) {}

    func makeCoordinator() -> Coordinator {
        Coordinator(tabId: tabId, viewModel: viewModel)
    }

    /// Custom UIView Subcategory, hold delegate Quoted to prevent release
    private class ObserverView: UIView {
        weak var delegate: Coordinator?
    }

    class Coordinator: NSObject, UIAdaptivePresentationControllerDelegate {
        let tabId: UUID
        let viewModel: BrowserViewModel

        init(tabId: UUID, viewModel: BrowserViewModel) {
            self.tabId = tabId
            self.viewModel = viewModel
        }

        /// The sign starts off and then it starts. sheet The image is still full.
        func presentationControllerWillDismiss(_ presentationController: UIPresentationController) {
            // RN Skip exit process when pre-screened
            guard !viewModel.skipNextDisappearSnapshot else {
                viewModel.skipNextDisappearSnapshot = false
                return
            }
            guard let tab = viewModel.tabs.first(where: { $0.id == tabId }),
                  !tab.title.isEmpty else {
                return
            }
            captureSnapshotSync(for: tab)
        }

        private func captureSnapshotSync(for tab: Tab) {
            guard let window = UIApplication.shared.connectedScenes
                .filter({ $0.activationState == .foregroundActive })
                .compactMap({ $0 as? UIWindowScene })
                .first?.windows
                .filter({ $0.isKeyWindow }).first else {
                return
            }

            let renderer = UIGraphicsImageRenderer(bounds: window.bounds)
            let fullImage = renderer.image { _ in
                window.drawHierarchy(in: window.bounds, afterScreenUpdates: false)
            }

            let screenWidth = window.bounds.width
            let screenHeight = window.bounds.height
            let cropHeight = screenWidth * 0.54 - 56
            let cropY = (screenHeight - cropHeight) / 2

            let cropRect = CGRect(x: 0, y: cropY, width: screenWidth, height: cropHeight)

            if let croppedImage = fullImage.cropped(to: cropRect) {
                viewModel.setTabSnapshot(croppedImage, for: tab.id)
            } else {
                viewModel.setTabSnapshot(fullImage, for: tab.id)
            }
        }
    }
}

private extension UIView {
    /// Along responder chain Finds the belongings up UIViewController
    func findHostingViewController() -> UIViewController? {
        var responder: UIResponder? = self
        while let r = responder {
            if let vc = r as? UIViewController { return vc }
            responder = r.next
        }
        return nil
    }
}
