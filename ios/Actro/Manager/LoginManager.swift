import A1SDK
import Combine

// Login Type Count
enum LoginType {
    case apple
    case wechat
}

// Login Results Count
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
    
    // Authorise context, save RN Request Parameters
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
    
    // MARK: - Notify listening settings
    private func setupNotificationObservers() {
        // Listen to apple login results
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleAppleLoginResult(_:)),
            name: .appleLoginResult,
            object: nil
        )
        
        // Listen to micro-information results
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleWeChatLoginResult(_:)),
            name: .wechatLoginResult,
            object: nil
        )
    }
    
    // MARK: - SDK Environment Configuration
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

    
    // MARK: - Public access
    func register() {
        weChatManager.registerApp()
    }
    
    /// Sets the context of the authorization (call before trigger login)
    func setAuthContext(_ context: [String: String]) {
        self.authContext = context
        logger.debug("[LoginManager] Set Mandate Context: \(context)")
    }
        
    /// Login
    func loginWithType(_ type: LoginType) {
        loginType = type
        if loginType == .wechat {
            weChatManager.sendLoginRequest()
        } else if loginType == .apple {
            appleLoginManager.signInWithApple()
        }
    }
    
    /// Logout
    func logout() {
        DispatchQueue.main.async {
            self.isLoggedIn = false
            self.currentUser = nil
            self.loginType = nil
            self.clearGlobalUserInfo()
        }
    }
    
    /// Write-off accounts
    func deleteAccount() {
        logout()
        // A1SDK.shared.auth.deleteAccount { ... } // Remove A1SDK Call
    }
    
    func handleOpenURL(_ url: URL, type: LoginType = .wechat) -> Bool {
        if type == .wechat {
            return weChatManager.handleOpenURL(url)
        } else {
            return false
        }
    }
    
    /// Check Apple Login Status
    func checkAppleLoginStatus(userID: String) {
        AppleLoginManager.shared.checkAppleIDCredentialState(userID: userID) { state in
            logger.debug("Apple Login State:\(state)")
        }
    }
    
    // MARK: - Channel Authorization Method
    
    /// Verify if user authorization is obtained
    /// - Parameters:
    ///   - appId: Apply ID
    ///   - userId: User ID (Usually. user.cn)
    ///   - appUrl: Apply URL
    ///   - completion: Reconverted and returned authorized
    func checkUserAuthorization(appId: String, userId: String, appUrl: String, completion: @escaping (Bool) -> Void) {
        guard let fullUrl = AuthCoreBridge.accessControlURL(appUrl: appUrl, appId: appId, userId: userId) else {
            logger.error("[LoginManager] checkUserAuthorization: Could not close temporary folder: %s appUrl: \(appUrl)")
            completion(false)
            return
        }

        logger.info("[LoginManager] Authenticate user authorization: \(fullUrl)")
        
        guard let requestUrl = URL(string: fullUrl) else {
            logger.error("[LoginManager] Invalid URL: \(fullUrl)")
            completion(false)
            return
        }
        
        var request = URLRequest(url: requestUrl)
        request.httpMethod = "GET"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        
        URLSession.shared.dataTask(with: request) { data, response, error in
            if let error = error {
                logger.error("[LoginManager] ❌ Could not close temporary folder: %s: \(error.localizedDescription)")
                completion(false)
                return
            }
            
            guard let data = data else {
                logger.error("[LoginManager] ❌ Verifying authorized response data as empty")
                completion(false)
                return
            }
            
            do {
                // Parsing Response JSON
                if let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
                   let code = json["code"] as? Int {
                    let isAuthorized = AuthCoreBridge.isAuthorizedCode(code)
                    logger.info("[LoginManager] \(isAuthorized ? "✅" : "❌") User Authorization Status: code=\(code), \(isAuthorized ? "Authorized" : "Unauthorized")")
                    completion(isAuthorized)
                } else {
                    logger.error("[LoginManager] ❌ Could not close temporary folder: %s")
                    completion(false)
                }
            } catch {
                logger.error("[LoginManager] ❌ Failed to parse authorized response: \(error.localizedDescription)")
                completion(false)
            }
        }.resume()
    }
    
    // MARK: - Private methods
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
        // TODO: Clear User Data
    }

    // MARK: - Local Storage Support Method
    /// Will UserDetailData Sequenced and saved to local databases (%1)UserInfoManager)
    func saveUserToLocal() {}

    // MARK: - Global user information (used to determine differences) appId (Accessors are consistent)
    private let globalUserInfoKeyPrefix = "Actro.globalUserInfo"
    
    /// Generate global user information storage based on the current environment key
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
            logger.debug("UserDetailData Sequencing failed: \(error), with a description string backup")
            userJsonString = String(describing: user)
        }
        let key = globalUserInfoKey
        UserDefaults.standard.set(userJsonString, forKey: key)
        logger.debug("Global user information saved to environment \(self.currentEnvironment): \(key)")
    }
    
    /// Remove global user information from the current environment
    private func clearGlobalUserInfo() {
        // A1SDK.shared.auth.logout() // Remove A1SDK Call
        let key = globalUserInfoKey
        UserDefaults.standard.removeObject(forKey: key)
        UserInfoManager.shared.deleteUser()
        logger.debug("Cleared global user information, environment: \(self.currentEnvironment), key: \(key)")
    }
    
    /// Remove global user information from the specified environment
    private func clearGlobalUserInfoWithEnv(_ envSuffix: String) {
        let key = globalUserInfoKeyPrefix + envSuffix
        UserDefaults.standard.removeObject(forKey: key)
        logger.debug("Cleared specified global user information, environment: \(envSuffix), key: \(key)")
    }
    
    /// Remove global user information for all settings
    func clearAllEnvironmentsUserInfo() {
        // A1SDK.shared.auth.logout() // Remove A1SDK Call
        
        // Remove user information for all settings
        let environments: [A1Environment] = [.dev, .beta, .prod]
        for env in environments {
            let envSuffix = storageSuffix(for: env)
            let key = "\(globalUserInfoKeyPrefix)\(envSuffix)"
            UserDefaults.standard.removeObject(forKey: key)
            logger.debug("Cleared Environment \(env) Global User Information, key: \(key)")
        }
        
        // Reset current login status
        DispatchQueue.main.async {
            self.currentUser = nil
            self.isLoggedIn = false
            self.loginType = nil
        }
        
        logger.debug("Cleared global user information for all environments")
    }

    /// Check Assign appId Do you think the local user information is consistent with the global user?
    func isAppUserConsistent(appId: String) -> Bool {
        guard let stored = UserInfoManager.shared.getUser(), !stored.userInfo.isEmpty else {
            // No local user information is considered inconsistent (authorization required)
            return false
        }
        guard let globalUser = currentUser else {
            // No global fingerprints are considered inconsistent (authorization required)
            return false
        }
        return stored.userIdentity == globalUser.user.cn
    }

    /// Call on application switch: if appId Unconsistent with global users triggers a re-authorization process
    func handleAppSwitch(to appId: String) {
        DispatchQueue.global(qos: .userInitiated).async {
            if !self.isLoggedIn {
                logger.debug("User is not login, not checking for authorization appid: \(appId)")
                return
            }
            let consistent = self.isAppUserConsistent(appId: appId)
            logger.debug("Inspection appId:\(appId) Users and global users: \(consistent)")
        }
    }

    /// Load the current from locally appId Users (no settings) global currentUser) for comparison or display where necessary
    func loadSavedUser() {
        if let stored = UserInfoManager.shared.getUser() {
            logger.debug("Loaded Local User Records (not set as global currentUser), userInfo Length: \(stored.userInfo.count)")
        }
    }

    /// From UserDefaults Load global user information (used for currentUser)
    private func loadGlobalUser() {
        let key = globalUserInfoKey
        guard let userJson = UserDefaults.standard.string(forKey: key), !userJson.isEmpty else {
            logger.debug("Global user information not found, environment: \(self.currentEnvironment), key: \(key), reset login status")
            // No user information found, reset login status
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
                logger.debug("Recovery from global cache currentUserEnvironment: \(self.currentEnvironment)")
            } catch {
                logger.debug("Failed to parse the global user information: \(error), reset login status")
                // Parsing failed, reset login status
                DispatchQueue.main.async {
                    self.currentUser = nil
                    self.isLoggedIn = false
                    self.loginType = nil
                }
            }
        }
    }
    
    // MARK: - Notification of method of treatment
    @objc private func handleAppleLoginResult(_ notification: Notification) {
        guard let result = notification.object as? [String: Any] else { return }
        
        if let success = result["success"] as? Bool, success {
            // Apple Login Success
            logger.debug("Apple Login successful, notification RN")
            if let identityToken = result["identityToken"] as? String {
                // Announcements RN
                UserInfoModule.notifyLoginResult([
                    LoginResultPayloadContract.type: LoginResultPayloadContract.appleType,
                    LoginResultPayloadContract.idToken: identityToken
                ])
            }
        } else {
            let errorMsg = result["errorMsg"] as? String ?? "Unknown error"
            logger.debug("Apple Login failed:\(errorMsg)")
        }
    }
    
    @objc private func handleWeChatLoginResult(_ notification: Notification) {
        guard let result = notification.object as? [String: Any] else { return }
        
        if let code = result[LoginResultPayloadContract.code] as? String, !code.isEmpty {
            logger.debug("We're good at logging in. RN, code: \(code)")
            UserInfoModule.notifyLoginResult([
                LoginResultPayloadContract.type: LoginResultPayloadContract.wechatType,
                LoginResultPayloadContract.code: code
            ])
        } else {
            let errorMsg = result["errorMsg"] as? String ?? "Unknown error"
            let errorCode = result["errorCode"]
            logger.debug("Could not close temporary folder: %s\(errorMsg), Error code:\(String(describing: errorCode))")
            
            // Announcements RN Login failed, clear easily loading Status or error reporting
            UserInfoModule.notifyLoginResult([
                LoginResultPayloadContract.type: LoginResultPayloadContract.wechatType,
                LoginResultPayloadContract.error: true,
                LoginResultPayloadContract.message: errorMsg
            ])
        }
    }
    
}
