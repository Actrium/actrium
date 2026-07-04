import A1SDK
import Combine

// 登录类型枚举
enum LoginType {
    case apple
    case wechat
}

// 登录结果枚举
enum LoginResult {
    case success(userInfo: [String: Any])
    case failure(error: String)
    case cancelled
}

private let logger = AppLogger.app

class LoginManager: ObservableObject {
    static let shared = LoginManager()
    
    @Published var isLoggedIn: Bool = false
    @Published var currentUser: UserDetailData?
    @Published var loginType: LoginType?
    
    var userEnvironment: A1Environment? = nil
    var currentEnvironment: A1Environment = .beta
    
    // 授权上下文，保存 RN 请求参数
    private var authContext: [String: String]? = nil
        
    private let weChatManager = WeChatManager.shared
    private let appleLoginManager = AppleLoginManager.shared
    
    private init() {
        setupNotificationObservers()
        setupDefaultConfig()
        loadGlobalUser()
    }
    
    deinit {
        NotificationCenter.default.removeObserver(self)
    }
    
    // MARK: - 通知监听设置
    private func setupNotificationObservers() {
        // 监听苹果登录结果
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleAppleLoginResult(_:)),
            name: .appleLoginResult,
            object: nil
        )
        
        // 监听微信登录结果
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleWeChatLoginResult(_:)),
            name: .wechatLoginResult,
            object: nil
        )
    }
    
    // MARK: - SDK 环境配置
    func setupConfig(_ url: String) {
        let environment = environmentForAppUrl(url)
        currentEnvironment = environment
        if userEnvironment == nil {
            userEnvironment = environment
            loadGlobalUser()
        }
        
        A1SDK.shared.configure(
            environment: environment,
            logLevel: .debug,
            enableLogging: true
        )
    }
    
    func environmentForUrl(_ url: String) -> A1Environment {
        a1Environment(from: AuthCoreBridge.environment(from: url))
    }

    func setupDefaultConfig() {
        A1SDK.shared.configure(
            environment: currentEnvironment,
            logLevel: .debug,
            enableLogging: true
        )
    }

    
    // MARK: - 公共登录方法
    func register() {
        weChatManager.registerApp()
    }
    
    /// 设置授权上下文（在触发登录前调用）
    func setAuthContext(_ context: [String: String]) {
        self.authContext = context
        logger.debug("[LoginManager] 设置授权上下文: \(context)")
    }
        
    /// 登录
    func loginWithType(_ type: LoginType) {
        loginType = type
        if loginType == .wechat {
            weChatManager.sendLoginRequest()
        } else if loginType == .apple {
            appleLoginManager.signInWithApple()
        }
    }
    
    /// 登出
    func logout() {
        DispatchQueue.main.async {
            self.isLoggedIn = false
            self.currentUser = nil
            self.loginType = nil
            self.clearGlobalUserInfo()
        }
    }
    
    /// 注销账户
    func deleteAccount() {
        logout()
        // A1SDK.shared.auth.deleteAccount { ... } // 移除 A1SDK 调用
    }
    
    func handleOpenURL(_ url: URL, type: LoginType = .wechat) -> Bool {
        if type == .wechat {
            return weChatManager.handleOpenURL(url)
        } else {
            return false
        }
    }
    
    /// 检查苹果登录状态
    func checkAppleLoginStatus(userID: String) {
        AppleLoginManager.shared.checkAppleIDCredentialState(userID: userID) { state in
            logger.debug("苹果登录状态：\(state)")
        }
    }
    
    // MARK: - 频道授权方法
    
    /// 验证用户是否已授权
    /// - Parameters:
    ///   - appId: 应用 ID
    ///   - userId: 用户 ID (通常是 user.cn)
    ///   - appUrl: 应用 URL
    ///   - completion: 完成回调，返回是否已授权
    func checkUserAuthorization(appId: String, userId: String, appUrl: String, completion: @escaping (Bool) -> Void) {
        guard let fullUrl = AuthCoreBridge.accessControlURL(appUrl: appUrl, appId: appId, userId: userId) else {
            logger.error("[LoginManager] checkUserAuthorization: 无法解析 appUrl: \(appUrl)")
            completion(false)
            return
        }

        logger.info("[LoginManager] 验证用户授权: \(fullUrl)")
        
        guard let requestUrl = URL(string: fullUrl) else {
            logger.error("[LoginManager] 无效的 URL: \(fullUrl)")
            completion(false)
            return
        }
        
        var request = URLRequest(url: requestUrl)
        request.httpMethod = "GET"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        
        URLSession.shared.dataTask(with: request) { data, response, error in
            if let error = error {
                logger.error("[LoginManager] ❌ 验证授权失败: \(error.localizedDescription)")
                completion(false)
                return
            }
            
            guard let data = data else {
                logger.error("[LoginManager] ❌ 验证授权响应数据为空")
                completion(false)
                return
            }
            
            do {
                // 解析响应 JSON
                if let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
                   let code = json["code"] as? Int {
                    let isAuthorized = AuthCoreBridge.isAuthorizedCode(code)
                    logger.info("[LoginManager] \(isAuthorized ? "✅" : "❌") 用户授权状态: code=\(code), \(isAuthorized ? "已授权" : "未授权")")
                    completion(isAuthorized)
                } else {
                    logger.error("[LoginManager] ❌ 无法解析授权响应")
                    completion(false)
                }
            } catch {
                logger.error("[LoginManager] ❌ 解析授权响应失败: \(error.localizedDescription)")
                completion(false)
            }
        }.resume()
    }
    
    // MARK: - 私有方法
    private func environmentForAppUrl(_ url: String) -> A1Environment {
        a1Environment(from: AuthCoreBridge.environment(fromAppUrl: url))
    }

    private func a1Environment(from environment: String) -> A1Environment {
        switch environment {
        case AuthEnvironmentContract.dev:
            return .dev
        case AuthEnvironmentContract.beta:
            return .beta
        default:
            return .prod
        }
    }

    private func authEnvironmentName(for environment: A1Environment) -> String {
        switch environment {
        case .dev:
            return AuthEnvironmentContract.dev
        case .beta:
            return AuthEnvironmentContract.beta
        case .prod:
            return AuthEnvironmentContract.prod
        @unknown default:
            return AuthEnvironmentContract.beta
        }
    }

    private func storageSuffix(for environment: A1Environment) -> String {
        AuthCoreBridge.storageSuffix(for: authEnvironmentName(for: environment))
    }
    
    private func clearUserData() {
        // TODO: 清除用户数据
    }

    // MARK: - 本地存储辅助方法
    /// 将 UserDetailData 序列化并保存到本地数据库（UserInfoManager）
    func saveUserToLocal() {}

    // MARK: - 全局用户信息（用于判定不同 appId 下用户是否一致）
    private let globalUserInfoKeyPrefix = "Actro.globalUserInfo"
    
    /// 根据当前环境生成全局用户信息的存储 key
    private var globalUserInfoKey: String {
        "\(globalUserInfoKeyPrefix)\(storageSuffix(for: currentEnvironment))"
    }

    private func saveGlobalUserInfo(_ user: UserDetailData) {
        var userJsonString: String = ""
        do {
            let encoder = JSONEncoder()
            encoder.dateEncodingStrategy = .iso8601
            let data = try encoder.encode(user)
            userJsonString = String(data: data, encoding: .utf8) ?? String(describing: user)
        } catch {
            logger.debug("UserDetailData 序列化失败: \(error)，使用描述字符串备份")
            userJsonString = String(describing: user)
        }
        let key = globalUserInfoKey
        UserDefaults.standard.set(userJsonString, forKey: key)
        logger.debug("已保存全局用户信息到环境 \(self.currentEnvironment): \(key)")
    }
    
    /// 删除当前环境的全局用户信息
    private func clearGlobalUserInfo() {
        // A1SDK.shared.auth.logout() // 移除 A1SDK 调用
        let key = globalUserInfoKey
        UserDefaults.standard.removeObject(forKey: key)
        UserInfoManager.shared.deleteUser()
        logger.debug("已清除全局用户信息，环境: \(self.currentEnvironment), key: \(key)")
    }
    
    /// 删除指定环境的全局用户信息
    private func clearGlobalUserInfoWithEnv(_ envSuffix: String) {
        let key = globalUserInfoKeyPrefix + envSuffix
        UserDefaults.standard.removeObject(forKey: key)
        logger.debug("已清除指定全局用户信息，环境: \(envSuffix), key: \(key)")
    }
    
    /// 删除所有环境的全局用户信息
    func clearAllEnvironmentsUserInfo() {
        // A1SDK.shared.auth.logout() // 移除 A1SDK 调用
        
        // 删除所有环境的用户信息
        let environments: [A1Environment] = [.dev, .beta, .prod]
        for env in environments {
            let envSuffix = storageSuffix(for: env)
            let key = "\(globalUserInfoKeyPrefix)\(envSuffix)"
            UserDefaults.standard.removeObject(forKey: key)
            logger.debug("已清除环境 \(env) 的全局用户信息, key: \(key)")
        }
        
        // 重置当前登录状态
        DispatchQueue.main.async {
            self.currentUser = nil
            self.isLoggedIn = false
            self.loginType = nil
        }
        
        logger.debug("已清除所有环境的全局用户信息")
    }

    /// 检查指定 appId 的本地用户信息是否和全局用户一致
    func isAppUserConsistent(appId: String) -> Bool {
        guard let stored = UserInfoManager.shared.getUser(), !stored.userInfo.isEmpty else {
            // 没有本地用户信息视为不一致（需要授权）
            return false
        }
        guard let globalUser = currentUser else {
            // 没有全局指纹视为不一致（需要授权）
            return false
        }
        return stored.userIdentity == globalUser.user.cn
    }

    /// 在应用切换时调用：若 appId 对应用户与全局用户不一致，则触发重新授权流程
    func handleAppSwitch(to appId: String) {
        DispatchQueue.global(qos: .userInitiated).async {
            if !self.isLoggedIn {
                logger.debug("用户未登录，不检测是否授权 appid：\(appId)")
                return
            }
            let consistent = self.isAppUserConsistent(appId: appId)
            logger.debug("检查 appId:\(appId) 的用户与全局用户一致性: \(consistent)")
        }
    }

    /// 从本地加载当前 appId 的用户（不设置 global currentUser），用于必要时比较或展示
    func loadSavedUser() {
        if let stored = UserInfoManager.shared.getUser() {
            logger.debug("加载到的本地用户记录（未设为 global currentUser），userInfo 长度: \(stored.userInfo.count)")
        }
    }

    /// 从 UserDefaults 加载全局用户信息（用于 currentUser）
    private func loadGlobalUser() {
        let key = globalUserInfoKey
        guard let userJson = UserDefaults.standard.string(forKey: key), !userJson.isEmpty else {
            logger.debug("未找到全局用户信息，环境: \(self.currentEnvironment), key: \(key)，重置登录状态")
            // 未找到用户信息，重置登录状态
            DispatchQueue.main.async {
                self.currentUser = nil
                self.isLoggedIn = false
                self.loginType = nil
            }
            return
        }
        if let data = userJson.data(using: .utf8) {
            do {
                let decoder = JSONDecoder()
                decoder.dateDecodingStrategy = .iso8601
                let user = try decoder.decode(UserDetailData.self, from: data)
                DispatchQueue.main.async {
                    self.currentUser = user
                    self.isLoggedIn = true
                    self.loginType = user.profiles.first?.identityType == .wechat ? .wechat : .apple
                }
                logger.debug("已从全局缓存恢复 currentUser，环境: \(self.currentEnvironment)")
            } catch {
                logger.debug("解析全局用户信息失败: \(error)，重置登录状态")
                // 解析失败，重置登录状态
                DispatchQueue.main.async {
                    self.currentUser = nil
                    self.isLoggedIn = false
                    self.loginType = nil
                }
            }
        }
    }
    
    // MARK: - 通知处理方法
    @objc private func handleAppleLoginResult(_ notification: Notification) {
        guard let result = notification.object as? [String: Any] else { return }
        
        if let success = result["success"] as? Bool, success {
            // 苹果登录成功
            logger.debug("Apple 登录成功，通知 RN")
            if let identityToken = result["identityToken"] as? String {
                // 通知 RN
                UserInfoModule.notifyLoginResult([
                    LoginResultPayloadContract.type: LoginResultPayloadContract.appleType,
                    LoginResultPayloadContract.idToken: identityToken
                ])
            }
        } else {
            let errorMsg = result["errorMsg"] as? String ?? "未知错误"
            logger.debug("Apple 登录失败：\(errorMsg)")
        }
    }
    
    @objc private func handleWeChatLoginResult(_ notification: Notification) {
        guard let result = notification.object as? [String: Any] else { return }
        
        if let code = result[LoginResultPayloadContract.code] as? String, !code.isEmpty {
            logger.debug("微信登录成功，通知 RN, code：\(code)")
            UserInfoModule.notifyLoginResult([
                LoginResultPayloadContract.type: LoginResultPayloadContract.wechatType,
                LoginResultPayloadContract.code: code
            ])
        } else {
            let errorMsg = result["errorMsg"] as? String ?? "未知错误"
            let errorCode = result["errorCode"]
            logger.debug("微信登录失败：\(errorMsg), 错误码：\(String(describing: errorCode))")
            
            // 通知 RN 登录失败，方便清除 loading 状态或报错
            UserInfoModule.notifyLoginResult([
                LoginResultPayloadContract.type: LoginResultPayloadContract.wechatType,
                LoginResultPayloadContract.error: true,
                LoginResultPayloadContract.message: errorMsg
            ])
        }
    }
    
}
