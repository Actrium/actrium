import Foundation
import React
import A1SDK

private let logger = AppLogger.reactNative

/// Native module for user info events - Swift → RN EventEmitter
@objc(UserInfoModule)
class UserInfoModule: RCTEventEmitter {
    
    // 单例引用，便于从其他地方发送事件
    private static weak var shared: UserInfoModule?
    
    // 等待登录完成的请求队列
    private var pendingRequests: [(params: [String: Any], resolve: RCTPromiseResolveBlock, reject: RCTPromiseRejectBlock)] = []
    
    override init() {
        super.init()
        UserInfoModule.shared = self
        logger.notice("[UserInfoModule] Initialized")
    }
    
    deinit {
        logger.notice("[UserInfoModule] Deinitialized")
    }
    
    // MARK: - RCTEventEmitter
    
    /// 声明支持的事件列表
    override func supportedEvents() -> [String]! {
        return [
            "onUserInfoChanged",
            "onLoginStatusChanged",
            "onUserLogout",
            "onLoginResult"
        ]
    }
    
    /// 触发登录
    @objc
    func loginWithType(_ type: String) {
        let loginType: LoginType = type == "wechat" ? .wechat : .apple
        DispatchQueue.main.async {
            LoginManager.shared.loginWithType(loginType)
        }
    }
    
    /// 确保在主队列初始化
    @objc
    override static func requiresMainQueueSetup() -> Bool {
        return true
    }
    
    // MARK: - Public Static Methods (从外部调用发送事件)
    
    /// 发送用户信息变更事件
    @objc
    static func notifyUserInfoChanged(_ userInfo: [String: Any]) {
        guard let shared = shared else {
            logger.warning("[UserInfoModule] Module not initialized, cannot send event")
            return
        }
        
        logger.info("[UserInfoModule] Sending onUserInfoChanged event")
        shared.sendEvent(withName: "onUserInfoChanged", body: userInfo)
    }
    
    /// 发送登录状态变更事件
    @objc
    static func notifyLoginStatusChanged(_ status: [String: Any]) {
        guard let shared = shared else {
            logger.warning("[UserInfoModule] Module not initialized, cannot send event")
            return
        }
        
        logger.info("[UserInfoModule] Sending onLoginStatusChanged event")
        shared.sendEvent(withName: "onLoginStatusChanged", body: status)
    }
    
    /// 发送用户登出事件
    @objc
    static func notifyUserLogout() {
        guard let shared = shared else {
            logger.warning("[UserInfoModule] Module not initialized, cannot send event")
            return
        }
        
        logger.info("[UserInfoModule] Sending onUserLogout event")
        shared.sendEvent(withName: "onUserLogout", body: nil)
    }

    /// 发送登录结果事件 (wechat code / apple idToken)
    @objc
    static func notifyLoginResult(_ result: [String: Any]) {
        guard let shared = shared else {
            logger.warning("[UserInfoModule] Module not initialized, cannot send event")
            return
        }
        
        logger.info("[UserInfoModule] Sending onLoginResult event")
        shared.sendEvent(withName: "onLoginResult", body: result)
    }
    
    // MARK: - Optional: Expose methods to RN (if needed)
    
    /// RN 主动请求当前用户信息（可选）
    /// - Parameters:
    ///   - params: 包含 clientId、appId、tabId、appUrl 的参数字典
    @objc
    func getCurrentUserInfo(
        _ params: [String: Any],
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        logger.debug("[UserInfoModule] RN requested current user info")
        logger.debug("[UserInfoModule] Params: \(params)")
        
        let appUrl = params["appUrl"] as? String ?? ""
        
        // 检查本地用户信息
        if UserInfoManager.shared.getUser() != nil {
            // 有本地用户信息，验证环境是否一致
            if !appUrl.isEmpty {
                let appEnvironment = mapUrlToEnvironment(appUrl)
                let currentEnvironment = LoginManager.shared.currentEnvironment
                
                if appEnvironment != currentEnvironment {
                    logger.warning("[UserInfoModule] Environment mismatch: app=\(appEnvironment), current=\(currentEnvironment)")
                    logger.info("[UserInfoModule] Triggering re-login due to environment change")
                    
                    // 构建授权上下文
                    let authContext: [String: String] = [
                        "clientId": params["clientId"] as? String ?? "",
                        "appId": params["appId"] as? String ?? "",
                        "tabId": params["tabId"] as? String ?? "",
                        "appUrl": appUrl
                    ]
                    
                    // 环境不一致，触发重新登录
                    pendingRequests.append((params: params, resolve: resolve, reject: reject))
                    DispatchQueue.main.async {
                        LoginManager.shared.setupConfig(appUrl)
                        LoginManager.shared.setAuthContext(authContext)
                    }
                    return
                }
            }
            
            // 环境一致，验证用户是否已授权
            logger.debug("[UserInfoModule] Found local user info with matching environment, checking authorization")
            
            let appId = params["appId"] as? String ?? ""
            let clientId = params["clientId"] as? String ?? ""
            
            // 从本地用户信息中获取 userId
            guard let user = UserInfoManager.shared.getUser() else {
                logger.error("[UserInfoModule] Failed to get user info for authorization check")
                processUserInfoRequest(params: params, resolve: resolve, reject: reject)
                return
            }
            
            // 解析 userIdentity 获取 userId（假设 userIdentity 就是 user.cn）
            let userId = user.userIdentity
            
            // 验证用户授权
            LoginManager.shared.checkUserAuthorization(appId: appId, userId: userId, appUrl: appUrl) { [weak self] isAuthorized in
                guard let self = self else { return }
                
                if isAuthorized {
                    // 已授权，直接返回用户信息
                    logger.debug("[UserInfoModule] User is authorized, returning user info")
                    self.processUserInfoRequest(params: params, resolve: resolve, reject: reject)
                } else {
                    // 未授权，触发授权流程
                    logger.info("[UserInfoModule] User not authorized, triggering authorization flow")
                    
                    // 构建授权上下文
                    let authContext: [String: String] = [
                        "clientId": clientId,
                        "appId": appId,
                        "tabId": params["tabId"] as? String ?? "",
                        "appUrl": appUrl
                    ]
                    
                    // 将请求加入等待队列
                    self.pendingRequests.append((params: params, resolve: resolve, reject: reject))
                    
                    // 触发授权弹窗
                    DispatchQueue.main.async {
                        LoginManager.shared.setAuthContext(authContext)
                    }
                }
            }
            return
        }
        
        // 无本地用户信息，说明未登录，触发登录流程
        // 登录成功后会自动授权并保存到本地，不弹授权提示框
        logger.info("[UserInfoModule] No local user info, user not logged in, triggering login flow")
        pendingRequests.append((params: params, resolve: resolve, reject: reject))
        
        // 构建授权上下文
        let authContext: [String: String] = [
            "clientId": params["clientId"] as? String ?? "",
            "appId": params["appId"] as? String ?? "",
            "tabId": params["tabId"] as? String ?? "",
            "appUrl": appUrl
        ]
        
        // 设置环境、授权上下文并显示登录弹窗
        DispatchQueue.main.async {
            if !appUrl.isEmpty {
                LoginManager.shared.setupConfig(appUrl)
            }
            // 设置授权上下文，登录成功后会自动进行授权
            LoginManager.shared.setAuthContext(authContext)
        }
    }
    
    /// 将 URL 映射到环境枚举
    private func mapUrlToEnvironment(_ urlString: String) -> A1Environment {
        guard let url = URL(string: urlString),
              let host = url.host else {
            logger.error("[UserInfoModule] Failed to parse URL: \(urlString)")
            return .prod
        }
        
        if host.hasSuffix(".develenv.com") {
            return .dev
        } else if host.hasSuffix(".betaenv.com") {
            return .beta
        } else if host.hasSuffix(".askaway.chat") {
            return .prod
        } else {
            logger.warning("[UserInfoModule] Unknown domain: \(host), defaulting to prod")
            return .prod
        }
    }
    
    /// 处理用户信息请求（提取为独立方法以便复用）
    private func processUserInfoRequest(
        params: [String: Any],
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        let clientId = params["clientId"] as? String ?? ""
        let appId = params["appId"] as? String ?? ""
        let tabId = params["tabId"] as? String ?? ""
        let appUrl = params["appUrl"] as? String ?? ""
        
        logger.debug("[UserInfoModule] RN requested current user info")
        logger.debug("[UserInfoModule] Params - clientId: \(clientId), appId: \(appId), tabId: \(tabId), appUrl: \(appUrl)")
        
        if let user = UserInfoManager.shared.getUser() {
            let userDict: [String: Any] = [
                "id": user.id,
                "loginApiUrl": user.loginApiUrl,
                "userInfo": user.userInfo,
                "userIdentity": user.userIdentity,
                "sessionToken": user.sessionToken,
                "createdAt": user.createdAt.timeIntervalSince1970,
                "updatedAt": user.updatedAt.timeIntervalSince1970,
                // 回传请求参数，方便 RN 端关联
                "requestContext": [
                    "clientId": clientId,
                    "appId": appId,
                    "tabId": tabId,
                    "appUrl": appUrl
                ]
            ]
            resolve(userDict)
        } else {
            resolve(nil)
        }
    }
}
