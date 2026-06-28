import Combine

private let logger = AppLogger(category: "UserInfoManager")

// 存储的用户信息模型
struct UserStorageInfo: Codable, Identifiable {
    let id: Int64 // 固定为 1（单用户场景）
    var loginApiUrl: String // 当前登录的接口地址
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
        self.id = 1 // 单用户场景，固定 ID
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
    private let userInfoKey = "com.canvas.userInfo"
    private let userDefaults = UserDefaults.standard

    private init() {
        logger.debug("UserInfoManager 初始化（使用 UserDefaults）")
    }

    // MARK: - UserDefaults Storage

    /// 保存或更新用户信息到 UserDefaults
    @discardableResult
    func addOrUpdateUser(_ user: UserStorageInfo) -> Bool {
        logger.info("保存用户信息到 UserDefaults (API: \(user.loginApiUrl))")
        
        do {
            // 更新时间戳
            var updatedUser = user
            updatedUser.updatedAt = Date()
            
            // 序列化为 JSON
            let encoder = JSONEncoder()
            encoder.dateEncodingStrategy = .iso8601
            let data = try encoder.encode(updatedUser)
            
            // 保存到 UserDefaults
            userDefaults.set(data, forKey: userInfoKey)
            userDefaults.synchronize() // 立即同步
            
            logger.debug("用户信息已保存到 UserDefaults")
            
            // 发送事件到 RN
            notifyUserInfoToRN(updatedUser, isLogin: true)
            
            return true
        } catch {
            logger.error("保存用户信息失败: \(error.localizedDescription)")
            return false
        }
    }

    /// 从 UserDefaults 读取用户信息
    func getUser() -> UserStorageInfo? {
        guard let data = userDefaults.data(forKey: userInfoKey) else {
            logger.debug("UserDefaults 中没有用户信息")
            return nil
        }
        
        do {
            let decoder = JSONDecoder()
            decoder.dateDecodingStrategy = .iso8601
            let user = try decoder.decode(UserStorageInfo.self, from: data)
            logger.debug("从 UserDefaults 读取用户信息成功 (API: \(user.loginApiUrl))")
            return user
        } catch {
            logger.error("解析用户信息失败: \(error.localizedDescription)")
            return nil
        }
    }

    /// 从 UserDefaults 删除用户信息
    @discardableResult
    func deleteUser() -> Bool {
        logger.info("从 UserDefaults 删除用户信息")
        
        // 检查是否存在
        guard userDefaults.data(forKey: userInfoKey) != nil else {
            logger.warning("UserDefaults 中没有用户信息，无需删除")
            return false
        }
        
        // 删除
        userDefaults.removeObject(forKey: userInfoKey)
        userDefaults.synchronize()
        
        logger.debug("用户信息已从 UserDefaults 删除")
        
        // 发送登出事件到 RN
        notifyUserLogoutToRN()
        
        return true
    }
    
    /// 检查是否已登录（是否存在用户信息）
    func isLoggedIn() -> Bool {
        return userDefaults.data(forKey: userInfoKey) != nil
    }
    
    /// 清除所有用户数据（用于调试或重置）
    func clearAll() {
        logger.warning("清除所有用户数据")
        userDefaults.removeObject(forKey: userInfoKey)
        userDefaults.synchronize()
    }
    
    // MARK: - React Native Event Integration
    
    /// 发送用户信息到 RN（私有方法）
    private func notifyUserInfoToRN(_ user: UserStorageInfo, isLogin: Bool) {
        let userDict: [String: Any] = [
            "id": user.id,
            "loginApiUrl": user.loginApiUrl,
            "userInfo": user.userInfo,
            "userIdentity": user.userIdentity,
            "sessionToken": user.sessionToken,
            "createdAt": user.createdAt.timeIntervalSince1970,
            "updatedAt": user.updatedAt.timeIntervalSince1970
        ]
        
        #if canImport(React)
        // 通知 RN 用户信息变更
        UserInfoModule.notifyUserInfoChanged(userDict)
        
        // 通知登录状态
        if isLogin {
            UserInfoModule.notifyLoginStatusChanged(["isLoggedIn": true])
        }
        
        logger.debug("已发送用户信息到 RN (API: \(user.loginApiUrl))")
        #endif
    }
    
    /// 发送登出事件到 RN（私有方法）
    private func notifyUserLogoutToRN() {
        #if canImport(React)
        UserInfoModule.notifyUserLogout()
        UserInfoModule.notifyLoginStatusChanged(["isLoggedIn": false])
        logger.debug("已发送登出事件到 RN")
        #endif
    }
}

