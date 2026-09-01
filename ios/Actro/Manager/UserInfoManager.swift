import Combine

private let logger = AppLogger(category: "UserInfoManager")

// Storage of user information models
struct UserStorageInfo: Codable, Identifiable {
    let id: Int64 // Fixed as 1(single user scene)
    var loginApiUrl: String // Current login interface address
    var userInfo: String // JSON string of A1UserModels.UserInfo / UserDetailData
    var userIdentity: String // JSON string of identities
    var sessionToken: String
    var createdAt: Date
    var updatedAt: Date

    init(
        loginApiUrl: String,
        userInfo: String,
        userIdentity: String,
        sessionToken: String,
        createdAt: Date = Date(),
        updatedAt: Date = Date()
    ) {
        self.id = 1 // Single user scene, fixed ID
        self.loginApiUrl = loginApiUrl
        self.userInfo = userInfo
        self.userIdentity = userIdentity
        self.sessionToken = sessionToken
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

class UserInfoManager: ObservableObject {
    static let shared = UserInfoManager()
    
    // UserDefaults key
    private let userInfoKey = "com.actrium.actro.userInfo"
    private let userDefaults = UserDefaults.standard

    private init() {
        logger.debug("UserInfoManager Initialization (Use) UserDefaults)")
    }

    // MARK: - UserDefaults Storage

    /// Save or update user information to UserDefaults
    @discardableResult
    func addOrUpdateUser(_ user: UserStorageInfo) -> Bool {
        logger.info("Can not open message UserDefaults (API: \(user.loginApiUrl))")
        
        do {
            // Update Timetamp
            var updatedUser = user
            updatedUser.updatedAt = Date()
            
            // Sequence As JSON
            let encoder = JSONEncoder()
            encoder.dateEncodingStrategy = .iso8601
            let data = try encoder.encode(updatedUser)
            
            // Save To UserDefaults
            userDefaults.set(data, forKey: userInfoKey)
            userDefaults.synchronize() // Synchronize Now
            
            logger.debug("User information saved to UserDefaults")
            
            // Send Event To RN
            notifyUserInfoToRN(updatedUser, isLogin: true)
            
            return true
        } catch {
            logger.error("Could not close temporary folder: %s: \(error.localizedDescription)")
            return false
        }
    }

    /// From UserDefaults Read user information
    func getUser() -> UserStorageInfo? {
        guard let data = userDefaults.data(forKey: userInfoKey) else {
            logger.debug("UserDefaults No user information available")
            return nil
        }
        
        do {
            let decoder = JSONDecoder()
            decoder.dateDecodingStrategy = .iso8601
            let user = try decoder.decode(UserStorageInfo.self, from: data)
            logger.debug("From UserDefaults Successfully read user information (API: \(user.loginApiUrl))")
            return user
        } catch {
            logger.error("Could not close temporary folder: %s: \(error.localizedDescription)")
            return nil
        }
    }

    /// From UserDefaults Remove User Info
    @discardableResult
    func deleteUser() -> Bool {
        logger.info("From UserDefaults Remove User Info")
        
        // Check if there is any.
        guard userDefaults.data(forKey: userInfoKey) != nil else {
            logger.warning("UserDefaults No user information, no need to delete")
            return false
        }
        
        // Delete
        userDefaults.removeObject(forKey: userInfoKey)
        userDefaults.synchronize()
        
        logger.debug("User Information From UserDefaults Delete")
        
        // Send Logout Event To RN
        notifyUserLogoutToRN()
        
        return true
    }
    
    /// Check for login (use information available)
    func isLoggedIn() -> Bool {
        return userDefaults.data(forKey: userInfoKey) != nil
    }
    
    /// Clear all user data (for debugging or resetting)
    func clearAll() {
        logger.warning("Clear All User Data")
        userDefaults.removeObject(forKey: userInfoKey)
        userDefaults.synchronize()
    }
    
    // MARK: - React Native Event Integration
    
    /// Can not open message RN(private methods)
    private func notifyUserInfoToRN(_ user: UserStorageInfo, isLogin: Bool) {
        let userDict: [String: Any] = [
            UserInfoBridgeContract.keyId: user.id,
            UserInfoBridgeContract.keyLoginApiUrl: user.loginApiUrl,
            UserInfoBridgeContract.keyUserInfo: user.userInfo,
            UserInfoBridgeContract.keyUserIdentity: user.userIdentity,
            UserInfoBridgeContract.keySessionToken: user.sessionToken,
            UserInfoBridgeContract.keyCreatedAt: user.createdAt.timeIntervalSince1970,
            UserInfoBridgeContract.keyUpdatedAt: user.updatedAt.timeIntervalSince1970
        ]
        
        #if canImport(React)
        // Announcements RN User information changes
        UserInfoModule.notifyUserInfoChanged(userDict)
        
        // Notify login status
        if isLogin {
            UserInfoModule.notifyLoginStatusChanged([UserInfoBridgeContract.keyIsLoggedIn: true])
        }
        
        logger.debug("Can not open message RN (API: \(user.loginApiUrl))")
        #endif
    }
    
    /// Send Logout Event To RN(private methods)
    private func notifyUserLogoutToRN() {
        #if canImport(React)
        UserInfoModule.notifyUserLogout()
        UserInfoModule.notifyLoginStatusChanged([UserInfoBridgeContract.keyIsLoggedIn: false])
        logger.debug("Sending logout event to RN")
        #endif
    }
}
