import SwiftUI

struct SettingsView: View {
    @Environment(\.openURL) var openURL
    
    @StateObject private var loginManager = LoginManager.shared
    
    @State private var showUserDetail = false
    @State private var selectedDetent: PresentationDetent = .medium
    @State private var selectedDetail: ASSettingDetailType = .unknown
    @State private var showDetail = false
    @State private var showLogoutAlert = false
    
    var divider: some View {
        return Color(hex: "#404042")
            .padding(.leading, 16 + 36)
            .padding(.trailing, 16)
            .frame(height: 1)
    }
    
    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 20) {
                Text("设置")
                    .font(.system(size: 34, weight: .bold))
                    .foregroundColor(.white)
                    .padding(.horizontal, 18)
                
                VStack(spacing: 18) {
                    ASUserInfoSection(
                        isLoggedIn: loginManager.isLoggedIn,
                        currentUser: loginManager.currentUser,
                        onTap: {
                            if loginManager.isLoggedIn {
                                showUserDetail = true
                            } else {
                                loginManager.showLogin(.loginView)
                            }
                        }
                    )
                    
                    VStack(spacing: 8) {
                        SettingsRowView(iconName: "circle.lefthalf.filled.righthalf.striped.horizontal", title: "外观")
                        divider
                        SettingsRowView(iconName: "globe.asia.australia.fill", title: "语言")
                        divider
                        SettingsRowView(iconName: "checkmark.shield.fill", title: "隐私与安全")
                        divider
                        SettingsRowView(iconName: "questionmark.circle", title: "帮助")
                        divider
                        SettingsRowView(iconName: "ellipsis.bubble.fill", title: "反馈", action: sendMail)
                        divider
                        SettingsRowView(iconName: "as.logo", title: "AskAway", trailingText: Bundle.main.appVersion, action: navigateAbout)
                    }
                    .background(Color(hex: "#2C2C2D"))
                    .cornerRadius(26)
                }
                
                Spacer()
                
                if loginManager.isLoggedIn {
                    SettingsRowView(
                        iconName: "rectangle.portrait.and.arrow.right.fill",
                        title: "退出登录",
                        showChevron: false,
                        foregroundColor: Color(hex: "#FF453A"),
                        action: {
                            showLogoutAlert = true
                        }
                    )
                    .background(Color(hex: "#2C2C2D"))
                    .cornerRadius(26)
                }
            }
            .padding(EdgeInsets(top: 34, leading: 30, bottom: 27, trailing: 30))
            .cornerRadius(34)
            .background(Color.black)
            .navigationDestination(isPresented: $showDetail, destination: {
                ASSettingDetailView(type: selectedDetail).navigationBarHidden(true)
            })
            .navigationDestination(isPresented: $showUserDetail, destination: {
                ASUserDetailView()
            })
            .sheet(isPresented: $loginManager.showLoginSheet) {
                HStack {
                    switch loginManager.currentLoginSheetType {
                    case .login:
                        LoginSheet()
                            .presentationDetents([.height(341)])
                            .interactiveDismissDisabled()
                    case .loginAuth:
                        LoginAuthorSheet()
                            .presentationDetents([.height(367)])
                            .interactiveDismissDisabled()
                    case .loginView:
                        LoginView(detent: $selectedDetent)
                            .presentationDetents([.large], selection: $selectedDetent)
                            .interactiveDismissDisabled()
                            .ignoresSafeArea()
                    case .none:
                        EmptyView()
                    }
                }
            }
            .alert(TM.login.loginOutTitle(), isPresented: $showLogoutAlert) {
                Button(TM.login.loginOutCancel(), role: .cancel) {}
                Button(TM.login.loginOutConfirm(), role: .destructive) {
                    loginManager.logout()
                }
            }
        }
    }
}

extension SettingsView {
    private func sendMail() {
        // 构建 mailto URL，注意要进行 URL 编码
        let subject = TM.menu.teasingTopic().replacingOccurrences(of: "Mu Ai", with: "AskAway")
        let urlString = "mailto:support@askaway.chat?subject=\(subject)"
            .addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? ""
        
        appLogger.debug("吐个槽。 url： \(urlString)")
        if let url = URL(string: urlString) {
            openURL(url)
        }
    }
    
    private func navigateDetail() {
        
    }
    
    private func navigateAbout() {
        selectedDetail = .about
        showDetail = true
    }
}
