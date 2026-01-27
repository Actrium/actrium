import A1SDK
import CryptoKit
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

enum LoginSheetType {
    case none
    case login
    case loginAuth
    case loginView
}

private let logger = AppLogger.app

class LoginManager: ObservableObject {
    static let shared = LoginManager()
    
    @Published var isLoggedIn: Bool = false
    @Published var showLoginSheet: Bool = false
    @Published var currentLoginSheetType: LoginSheetType = .none
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
        let environment = mapUrlToA1(url)
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
    
    /// 通用登录成功处理
    func login(userInfo: UserDetailData) {
        if let env = userEnvironment?.rawValue {
            clearGlobalUserInfoWithEnv(env)
        }
        userEnvironment = currentEnvironment
        
        DispatchQueue.main.async {
            self.hideAllLoginViews()
            self.isLoggedIn = true
            self.currentUser = userInfo
            self.saveGlobalUserInfo(userInfo)
            
            // 自动授权：将全局用户信息保存到 UserInfoManager（本地存储）
            self.saveUserToLocalStorage(userInfo)
        }
    }
    
    /// 将 UserDetailData 保存到 UserInfoManager（自动授权）
    private func saveUserToLocalStorage(_ user: UserDetailData) {
        // 如果有授权上下文，先进行授权获取 sessionToken
        if let context = authContext,
           let clientId = context["clientId"],
           let appId = context["appId"],
           let appUrl = context["appUrl"],
           !clientId.isEmpty {
            logger.info("[LoginManager] 开始授权流程，clientId: \(clientId), appId: \(appId)")
            authorizeWith(clientId: clientId, appId: appId, appUrl: appUrl, user: user)
        } else {
            // 没有授权上下文，直接保存（不需要 sessionToken）
            logger.info("[LoginManager] 无授权上下文，直接保存用户信息")
            saveUserWithoutAuth(user: user)
        }
    }
    
    /// 保存用户信息（不进行授权）
    private func saveUserWithoutAuth(user: UserDetailData) {
        do {
            let encoder = JSONEncoder()
            encoder.dateEncodingStrategy = .iso8601
            let userData = try encoder.encode(user)
            let userJsonString = String(data: userData, encoding: .utf8) ?? ""
            
            let storageInfo = UserStorageInfo(
                loginApiUrl: currentEnvironment.rawValue,
                userInfo: userJsonString,
                userIdentity: user.user.cn,
                sessionToken: "" // 无 sessionToken
            )
            
            let success = UserInfoManager.shared.addOrUpdateUser(storageInfo)
            if success {
                logger.debug("✅ 用户信息已保存到本地存储")
            } else {
                logger.error("❌ 保存用户信息到本地存储失败")
            }
        } catch {
            logger.error("❌ 序列化用户信息失败: \(error.localizedDescription)")
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
        A1SDK.shared.auth.deleteAccount { result in
            switch result {
            case .success(let msg):
                logger.debug("账户已删除: \(msg)")
            case .failure(let err):
                logger.error("删除账户失败: \(err.localizedDescription)")
            }
        }
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
    
    /// 执行授权（公共方法，供 UI 层调用）
    /// 使用当前已登录的用户和保存的授权上下文执行授权流程
    func performAuthorization() {
        guard let user = currentUser else {
            logger.error("[LoginManager] 执行授权失败：当前没有登录用户")
            return
        }
        
        guard let context = authContext,
              let clientId = context["clientId"],
              let appId = context["appId"],
              let appUrl = context["appUrl"],
              !clientId.isEmpty else {
            logger.error("[LoginManager] 执行授权失败：授权上下文不完整")
            return
        }
        
        logger.info("[LoginManager] 开始执行授权流程")
        authorizeWith(clientId: clientId, appId: appId, appUrl: appUrl, user: user)
    }
    
    /// 为指定 app 进行授权，获取 sessionToken
    /// - Parameters:
    ///   - clientId: 客户端 ID
    ///   - appId: 应用 ID
    ///   - appUrl: 应用 URL
    ///   - user: 用户信息
    private func authorizeWith(clientId: String, appId: String, appUrl: String, user: UserDetailData) {
        guard let identity = user.profiles.first else {
            logger.error("[LoginManager] 授权失败：用户身份信息获取不到")
            saveUserWithoutAuth(user: user)
            return
        }
        
        let loginHint = "\(identity.id)@\(identity.domain)"
        logger.info("[LoginManager] 调用授权接口，clientId: \(clientId), loginHint: \(loginHint)")
        
        A1SDK.shared.auth.authorize(
            clientId: clientId,
            loginHint: loginHint
        ) { [weak self] result in
            guard let self = self else { return }
            
            switch result {
            case .success(let data):
                logger.info("[LoginManager] ✅ 授权成功，获得临时授权码")
                logger.debug("[LoginManager] 授权码: \(data.code), 重定向URI: \(data.redirectUri)")
                
                // 上传 code 到服务器
                self.uploadCode(code: data.code, appId: appId, appUrl: appUrl, user: user)
                
            case .failure(let error):
                logger.error("[LoginManager] ❌ 授权失败: \(error.localizedDescription)")
                // 授权失败，仍然保存用户信息（无 sessionToken）
                self.saveUserWithoutAuth(user: user)
            }
        }
    }

    /// 验证用户是否已授权
    /// - Parameters:
    ///   - appId: 应用 ID
    ///   - userId: 用户 ID (通常是 user.cn)
    ///   - appUrl: 应用 URL
    ///   - completion: 完成回调，返回是否已授权
    func checkUserAuthorization(appId: String, userId: String, appUrl: String, completion: @escaping (Bool) -> Void) {
        guard let url = URL(string: appUrl),
              let host = url.host,
              let scheme = url.scheme else {
            logger.error("[LoginManager] checkUserAuthorization: 无法解析 appUrl: \(appUrl)")
            completion(false)
            return
        }
        
        let base = "\(scheme)://\(host)"
        let path = "/api/access_control/\(appId)/\(userId)"
        let fullUrl = "\(base)\(path)"
        
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
                    let isAuthorized = (code == 0)
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
    
    /// 上传授权码到服务器，获取 sessionToken
    /// - Parameters:
    ///   - code: 授权码
    ///   - appId: 应用 ID
    ///   - appUrl: 应用 URL
    ///   - user: 用户信息
    private func uploadCode(code: String, appId: String, appUrl: String, user: UserDetailData) {
        guard let url = URL(string: appUrl),
              let host = url.host,
              let scheme = url.scheme else {
            logger.error("[LoginManager] uploadCode: 无法解析 appUrl: \(appUrl)")
            saveUserWithoutAuth(user: user)
            return
        }
        
        let base = "\(scheme)://\(host)"
        let path = "/api/client/app/\(appId)"
        let fullUrl = "\(base)\(path)"
        
        logger.info("[LoginManager] 上传授权码到: \(fullUrl)")
        
        // 使用 URLSession 上传授权码
        guard let requestUrl = URL(string: fullUrl) else {
            logger.error("[LoginManager] 无效的 URL: \(fullUrl)")
            saveUserWithoutAuth(user: user)
            return
        }
        
        var request = URLRequest(url: requestUrl)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        
        let body = ["code": code]
        do {
            request.httpBody = try JSONSerialization.data(withJSONObject: body)
        } catch {
            logger.error("[LoginManager] 序列化请求体失败: \(error.localizedDescription)")
            saveUserWithoutAuth(user: user)
            return
        }
        
        URLSession.shared.dataTask(with: request) { [weak self] data, response, error in
            guard let self = self else { return }
            
            if let error = error {
                logger.error("[LoginManager] ❌ 上传授权码失败: \(error.localizedDescription)")
                self.saveUserWithoutAuth(user: user)
                return
            }
            
            logger.info("[LoginManager] ✅ 授权码上传成功")
            
            // 从 A1SDK 获取 sessionToken
            let sessionToken = A1SDK.shared.getSessionToken() ?? ""
            logger.debug("[LoginManager] 获取到 sessionToken: \(sessionToken.isEmpty ? "(空)" : "(已获取)")")
            
            // 保存用户信息（包含 sessionToken）
            self.saveUserWithSessionToken(user: user, sessionToken: sessionToken)
            
        }.resume()
    }
    
    /// 保存用户信息（包含 sessionToken）
    private func saveUserWithSessionToken(user: UserDetailData, sessionToken: String) {
        do {
            let encoder = JSONEncoder()
            encoder.dateEncodingStrategy = .iso8601
            let userData = try encoder.encode(user)
            let userJsonString = String(data: userData, encoding: .utf8) ?? ""
            
            let storageInfo = UserStorageInfo(
                loginApiUrl: currentEnvironment.rawValue,
                userInfo: userJsonString,
                userIdentity: user.user.cn,
                sessionToken: sessionToken
            )
            
            let success = UserInfoManager.shared.addOrUpdateUser(storageInfo)
            if success {
                logger.debug("[LoginManager] ✅ 用户信息已授权并保存到本地存储（含 sessionToken）")
            } else {
                logger.error("[LoginManager] ❌ 保存用户信息到本地存储失败")
            }
            NotificationCenter.default.post(
                name: NSNotification.Name("LoginSuccessNotification"),
                object: nil
            )
        } catch {
            NotificationCenter.default.post(
                name: NSNotification.Name("LoginSuccessNotification"),
                object: nil
            )
            logger.error("[LoginManager] ❌ 序列化用户信息失败: \(error.localizedDescription)")
        }
    }
        
    // MARK: - UI 控制方法
    func showLogin(_ type: LoginSheetType) {
        currentLoginSheetType = type
        if type != .none {
            showLoginSheet = true
        }
    }
    
    func hideAllLoginViews() {
        showLoginSheet = false
        currentLoginSheetType = .none
    }
    
    // MARK: - 私有方法
    private func mapUrlToA1(_ path: String) -> A1Environment {
        guard let url = URL(string: path),
              let host = url.host,
              let appId = url.pathComponents.last,
              !appId.isEmpty && appId != "/" else {
            logger.error("解析 URL 失败. path: \(path)")
            return .prod
        }
        if host.hasSuffix(".actrium-dev.example") {
            return .dev
        } else if host.hasSuffix(".actrium-beta.example") {
            return .beta
        } else if host.hasSuffix(".actrium.example") {
            return .prod
        } else {
            logger.error("未知的域名: \(url)，使用生产环境")
            return .prod
        }
    }
    
    private func clearUserData() {
        // TODO: 清除用户数据
    }

    // MARK: - 本地存储辅助方法
    /// 将 UserDetailData 序列化并保存到本地数据库（UserInfoManager）
    func saveUserToLocal() {}

    // MARK: - 全局用户信息（用于判定不同 appId 下用户是否一致）
    private let globalUserInfoKeyPrefix = "AskAway.globalUserInfo"
    
    /// 根据当前环境生成全局用户信息的存储 key
    private var globalUserInfoKey: String {
        let envSuffix: String
        switch currentEnvironment {
        case .dev:
            envSuffix = ".dev"
        case .beta:
            envSuffix = ".beta"
        case .prod:
            envSuffix = ".prod"
        @unknown default:
            envSuffix = ".beta"
        }
        return "\(globalUserInfoKeyPrefix)\(envSuffix)"
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
        A1SDK.shared.auth.logout()
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
        A1SDK.shared.auth.logout()
        
        // 删除所有环境的用户信息
        let environments: [A1Environment] = [.dev, .beta, .prod]
        for env in environments {
            let envSuffix: String
            switch env {
            case .dev:
                envSuffix = ".dev"
            case .beta:
                envSuffix = ".beta"
            case .prod:
                envSuffix = ".prod"
            @unknown default:
                envSuffix = ".beta"
            }
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
            if !consistent {
                DispatchQueue.main.async {
                    // 标记需要授权（UI 层可展示授权弹窗或页）
                    self.showLogin(.loginAuth)
                }
            }
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
            logger.debug("Apple 登录成功，result：\(result)")
            if let identityToken = result["identityToken"] as? String {
                fetchAppleUserInfo(identityToken)
            }
        } else {
            let errorMsg = result["errorMsg"] as? String ?? "未知错误"
            logger.debug("Apple 登录失败：\(errorMsg)")
        }
    }
    
    @objc private func handleWeChatLoginResult(_ notification: Notification) {
        guard let result = notification.object as? [String: Any] else { return }
        let code = result["code"] as? String ?? ""
        let errorCode = result["errorCode"]
        
        if !code.isEmpty {
            logger.debug("微信登录成功，code：\(code)")
            // 这里用 code 向你的服务器换取 access_token 和用户信息
            fetchWeChatUserInfo(code: code)
        } else {
            logger.debug("微信登录失败，错误码：\(String(describing: errorCode))")
        }
    }
    
    /// 使用微信授权码获取用户信息
    private func fetchWeChatUserInfo(code: String) {
        A1SDK.shared.auth.loginWithWeChatAndGetUserInfo(code: code) { result in
            logger.debug("========= WeChat Login 获取用户信息：\(result)")
            switch result {
            case .success(let userDetail):
                logger.debug("✅ 微信登录成功")
                self.login(userInfo: userDetail)
            case .failure(let error):
                logger.debug("❌ 微信登录失败: \(error.localizedDescription)")
            }
        }
    }
    
    private func fetchAppleUserInfo(_ identityToken: String) {
        A1SDK.shared.auth.loginWithAppleAndGetUserInfo(identityToken: identityToken) { result in
            logger.debug("========= Apple Login 获取用户信息：\(result)")
            switch result {
            case .success(let userDetail):
                logger.debug("✅ 苹果登录成功")
                self.login(userInfo: userDetail)
            case .failure(let error):
                logger.debug("❌ 苹果登录失败: \(error.localizedDescription)")
            }
        }
    }
}
