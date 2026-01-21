import SwiftUI

struct SettingsView: View {
    var body: some View {
        ZStack {
            Color(.systemGray3).edgesIgnoringSafeArea(.all)

            VStack {
                Spacer().frame(height: 40)

                VStack(alignment: .leading, spacing: 20) {
                    Text("设置")
                        .font(.system(size: 34, weight: .bold))
                        .foregroundColor(.white)
                        .padding(.horizontal, 18)

                    VStack(spacing: 14) {
                        SettingsRowView(iconName: "person.circle", title: "Account")

                        VStack(spacing: 8) {
                            SettingsRowView(iconName: "paintbrush", title: "外观")
                            Divider().background(Color(.sRGB, white: 0.18, opacity: 1))
                            SettingsRowView(iconName: "globe", title: "语言")
                            Divider().background(Color(.sRGB, white: 0.18, opacity: 1))
                            SettingsRowView(iconName: "shield", title: "隐私与安全")
                            Divider().background(Color(.sRGB, white: 0.18, opacity: 1))
                            SettingsRowView(iconName: "questionmark.circle", title: "帮助")
                            Divider().background(Color(.sRGB, white: 0.18, opacity: 1))
                            SettingsRowView(iconName: "bubble.left.and.bubble.right", title: "反馈")
                            Divider().background(Color(.sRGB, white: 0.18, opacity: 1))
                            SettingsRowView(iconName: "app", title: "AskAway", trailingText: "V 0.0.0")
                        }
                        .background(Color(.sRGB, white: 0.07, opacity: 1))
                        .cornerRadius(14)
                        .padding(.horizontal, 8)
                        .padding(.vertical, 6)
                    }
                    .padding(.horizontal, 8)

                    Spacer()

                    // Logout button
                    Button(action: {
                        // TODO: implement logout action
                    }) {
                        HStack {
                            ZStack {
                                RoundedRectangle(cornerRadius: 16, style: .continuous)
                                    .fill(Color(.sRGB, white: 0.12, opacity: 1))
                                    .frame(width: 36, height: 36)
                                Image(systemName: "rectangle.portrait.and.arrow.right")
                                    .foregroundColor(Color(.systemRed))
                            }

                            Text("退出登录")
                                .foregroundColor(Color(.systemRed))
                                .font(.system(size: 16, weight: .semibold))

                            Spacer()
                        }
                        .padding()
                        .background(Color(.sRGB, white: 0.08, opacity: 1))
                        .cornerRadius(18)
                    }
                    .padding(.horizontal, 18)
                    .padding(.bottom, 26)
                }
                .frame(maxWidth: 420)
                .padding(.vertical, 14)
                .background(Color.black)
                .cornerRadius(22)
                .shadow(color: Color(.sRGB, white: 0, opacity: 0.25), radius: 8, x: 0, y: 6)
                .padding(.horizontal, 36)

                Spacer()
            }
        }
    }
}

struct SettingsView_Previews: PreviewProvider {
    static var previews: some View {
        SettingsView()
            .previewLayout(.sizeThatFits)
            .frame(width: 600, height: 900)
    }
}
