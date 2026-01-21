import SwiftUI

struct SettingsView: View {
    var divider: some View {
        return Color(hex: "#404042")
            .padding(.leading, 16 + 36)
            .padding(.trailing, 16)
            .frame(height: 1)
    }
    
    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("设置")
                .font(.system(size: 34, weight: .bold))
                .foregroundColor(.white)
                .padding(.horizontal, 18)
            
            VStack(spacing: 18) {
                ASUserInfoSection(
                    isLoggedIn: false,
                    currentUser: nil,
                    onTap: {
//                        if loginManager.isLoggedIn {
//                            showUserDetail = true
//                        } else {
//                            loginManager.showLogin(.loginView)
//                        }
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
                    SettingsRowView(iconName: "ellipsis.bubble.fill", title: "反馈")
                    divider
                    SettingsRowView(iconName: "as.logo", title: "AskAway", trailingText: "V 0.0.0")
                }
                .background(Color(hex: "#2C2C2D"))
                .cornerRadius(26)
            }
            
            Spacer()
            
            // Logout button
            SettingsRowView(
                iconName: "rectangle.portrait.and.arrow.right.fill",
                title: "退出登录",
                showChevron: false,
                foregroundColor: Color(hex: "#FF453A")
            )
            .background(Color(hex: "#2C2C2D"))
            .cornerRadius(26)
        }
        .padding(EdgeInsets(top: 34, leading: 30, bottom: 27, trailing: 30))
        .cornerRadius(34)
        .background(Color.black)
    }
}
