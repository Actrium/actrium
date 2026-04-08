import SwiftUI
import UIKit
import Combine

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

            // Bottom section - User only
            VStack(spacing: 0) {
                Divider()
                    .padding(.horizontal, 1)

                // User section
                UserSectionView()
            }
        }
    }
}

// MARK: - User Section View

private struct UserSectionView: View {
    @StateObject private var userManager = UserManager.shared
    @State private var isHovering: Bool = false
    @State private var showingLoginSheet: Bool = false
    @State private var showingUserMenu: Bool = false

    var body: some View {
        Group {
            if userManager.isLoggedIn {
                // Logged in state
                Button(action: {
                    showingUserMenu = true
                }) {
                    HStack(spacing: 8) {
                        // Avatar
                        if let avatarURL = userManager.currentUser?.avatarURL {
                            AsyncImage(url: avatarURL) { image in
                                image
                                    .resizable()
                                    .scaledToFill()
                            } placeholder: {
                                userAvatarPlaceholder
                            }
                            .frame(width: 28, height: 28)
                            .clipShape(Circle())
                        } else {
                            userAvatarPlaceholder
                        }

                        VStack(alignment: .leading, spacing: 1) {
                            Text(userManager.currentUser?.displayName ?? "User")
                                .font(.system(size: 12, weight: .medium))
                                .lineLimit(1)

                            Text(userManager.currentUser?.email ?? "")
                                .font(.system(size: 10))
                                .foregroundColor(.secondary)
                                .lineLimit(1)
                        }

                        Spacer()

                        Image(systemName: "ellipsis")
                            .font(.system(size: 12))
                            .foregroundColor(.secondary)
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .background(isHovering ? Color.accentColor.opacity(0.1) : Color.clear)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .onHover { hovering in
                    isHovering = hovering
                }
                .popover(isPresented: $showingUserMenu, arrowEdge: .top) {
                    UserMenuView(userManager: userManager)
                }
            } else {
                // Not logged in state
                Button(action: {
                    showingLoginSheet = true
                }) {
                    HStack(spacing: 8) {
                        Image(systemName: "person.circle")
                            .font(.system(size: 20))
                            .foregroundColor(.secondary)

                        Text("Sign In")
                            .font(.system(size: 13))

                        Spacer()
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 10)
                    .background(isHovering ? Color.accentColor.opacity(0.1) : Color.clear)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .onHover { hovering in
                    isHovering = hovering
                }
                .sheet(isPresented: $showingLoginSheet) {
                    LoginSheetView()
                }
            }
        }
    }

    private var userAvatarPlaceholder: some View {
        Circle()
            .fill(Color.accentColor.opacity(0.2))
            .frame(width: 28, height: 28)
            .overlay(
                Text(userManager.currentUser?.displayName.prefix(1).uppercased() ?? "U")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundColor(.accentColor)
            )
    }
}

// MARK: - User Menu View

private struct UserMenuView: View {
    @ObservedObject var userManager: UserManager
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // User info header
            HStack(spacing: 10) {
                if let avatarURL = userManager.currentUser?.avatarURL {
                    AsyncImage(url: avatarURL) { image in
                        image
                            .resizable()
                            .scaledToFill()
                    } placeholder: {
                        Circle()
                            .fill(Color.accentColor.opacity(0.2))
                    }
                    .frame(width: 40, height: 40)
                    .clipShape(Circle())
                } else {
                    Circle()
                        .fill(Color.accentColor.opacity(0.2))
                        .frame(width: 40, height: 40)
                        .overlay(
                            Text(userManager.currentUser?.displayName.prefix(1).uppercased() ?? "U")
                                .font(.system(size: 16, weight: .medium))
                                .foregroundColor(.accentColor)
                        )
                }

                VStack(alignment: .leading, spacing: 2) {
                    Text(userManager.currentUser?.displayName ?? "User")
                        .font(.system(size: 14, weight: .medium))

                    Text(userManager.currentUser?.email ?? "")
                        .font(.system(size: 12))
                        .foregroundColor(.secondary)
                }
            }
            .padding()

            Divider()

            // Menu items
            VStack(spacing: 0) {
                MenuItemButton(icon: "person", title: "Profile") {
                    // TODO: Open profile
                    dismiss()
                }

                MenuItemButton(icon: "gearshape", title: "Settings") {
                    // TODO: Open settings
                    dismiss()
                }

                Divider()
                    .padding(.vertical, 4)

                MenuItemButton(icon: "rectangle.portrait.and.arrow.right", title: "Sign Out", isDestructive: true) {
                    userManager.logout()
                    dismiss()
                }
            }
            .padding(.vertical, 4)
        }
        .frame(width: 220)
    }
}

private struct MenuItemButton: View {
    let icon: String
    let title: String
    var isDestructive: Bool = false
    let action: () -> Void

    @State private var isHovering: Bool = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 10) {
                Image(systemName: icon)
                    .font(.system(size: 13))
                    .foregroundColor(isDestructive ? .red : .primary)
                    .frame(width: 18)

                Text(title)
                    .font(.system(size: 13))
                    .foregroundColor(isDestructive ? .red : .primary)

                Spacer()
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .background(isHovering ? Color.accentColor.opacity(0.1) : Color.clear)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovering in
            isHovering = hovering
        }
    }
}

// MARK: - Login Sheet View

private struct LoginSheetView: View {
    @Environment(\.dismiss) private var dismiss
    @StateObject private var userManager = UserManager.shared
    @State private var email: String = ""
    @State private var password: String = ""
    @State private var isLoading: Bool = false
    @State private var errorMessage: String?

    var body: some View {
        VStack(spacing: 20) {
            // Header
            VStack(spacing: 8) {
                Image(systemName: "person.circle.fill")
                    .font(.system(size: 48))
                    .foregroundColor(.accentColor)

                Text("Sign In")
                    .font(.title2)
                    .fontWeight(.semibold)

                Text("Sign in to sync your bookmarks and settings")
                    .font(.caption)
                    .foregroundColor(.secondary)
                    .multilineTextAlignment(.center)
            }
            .padding(.top)

            // Form
            VStack(spacing: 12) {
                TextField("Email", text: $email)
                    .textFieldStyle(.roundedBorder)
                    .textContentType(.emailAddress)

                SecureField("Password", text: $password)
                    .textFieldStyle(.roundedBorder)
                    .textContentType(.password)

                if let error = errorMessage {
                    Text(error)
                        .font(.caption)
                        .foregroundColor(.red)
                }
            }
            .padding(.horizontal)

            // Actions
            VStack(spacing: 10) {
                Button(action: performLogin) {
                    if isLoading {
                        ProgressView()
                            .scaleEffect(0.8)
                    } else {
                        Text("Sign In")
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(email.isEmpty || password.isEmpty || isLoading)
                .frame(maxWidth: .infinity)

                Button("Cancel") {
                    dismiss()
                }
                .buttonStyle(.borderless)
            }
            .padding(.horizontal)
            .padding(.bottom)
        }
        .frame(width: 300)
        .fixedSize(horizontal: false, vertical: true)
    }

    private func performLogin() {
        isLoading = true
        errorMessage = nil

        // Simulate login - replace with actual auth logic
        DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
            // For demo: accept any email/password
            userManager.login(
                email: email,
                displayName: email.components(separatedBy: "@").first ?? "User"
            )
            isLoading = false
            dismiss()
        }
    }
}

// MARK: - User Manager

@MainActor
class UserManager: ObservableObject {
    static let shared = UserManager()

    @Published var isLoggedIn: Bool = false
    @Published var currentUser: User?

    struct User {
        let id: String
        let email: String
        let displayName: String
        let avatarURL: URL?
    }

    private init() {
        // Check for stored credentials on init
        loadStoredUser()
    }

    func login(email: String, displayName: String, avatarURL: URL? = nil) {
        currentUser = User(
            id: UUID().uuidString,
            email: email,
            displayName: displayName,
            avatarURL: avatarURL
        )
        isLoggedIn = true
        saveUser()
    }

    func logout() {
        currentUser = nil
        isLoggedIn = false
        clearStoredUser()
    }

    private func loadStoredUser() {
        if let email = UserDefaults.standard.string(forKey: "user.email"),
           let displayName = UserDefaults.standard.string(forKey: "user.displayName") {
            currentUser = User(
                id: UserDefaults.standard.string(forKey: "user.id") ?? UUID().uuidString,
                email: email,
                displayName: displayName,
                avatarURL: UserDefaults.standard.url(forKey: "user.avatarURL")
            )
            isLoggedIn = true
        }
    }

    private func saveUser() {
        guard let user = currentUser else { return }
        UserDefaults.standard.set(user.id, forKey: "user.id")
        UserDefaults.standard.set(user.email, forKey: "user.email")
        UserDefaults.standard.set(user.displayName, forKey: "user.displayName")
        if let avatarURL = user.avatarURL {
            UserDefaults.standard.set(avatarURL, forKey: "user.avatarURL")
        }
    }

    private func clearStoredUser() {
        UserDefaults.standard.removeObject(forKey: "user.id")
        UserDefaults.standard.removeObject(forKey: "user.email")
        UserDefaults.standard.removeObject(forKey: "user.displayName")
        UserDefaults.standard.removeObject(forKey: "user.avatarURL")
    }
}

#Preview {
    SidebarView(viewModel: BrowserViewModel(), historyVM: HistoryViewModel())
}
