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
            UserInfoBridgeContract.eventUserInfoChanged,
            UserInfoBridgeContract.eventLoginStatusChanged,
            UserInfoBridgeContract.eventUserLogout,
            LoginResultPayloadContract.eventName
        ]
    }
    
    /// 触发登录
    @objc
    func loginWithType(_ type: String) {
        let loginType: LoginType = type == LoginResultPayloadContract.wechatType ? .wechat : .apple
        DispatchQueue.main.async {
            LoginManager.shared.loginWithType(loginType)
        }
    }
    
    /// 确保在主队列初始化
    @objc
    override static func requiresMainQueueSetup() -> Bool {
        return true
    }

    private func stringParam(_ params: [String: Any], _ key: String) -> String {
        params[key] as? String ?? ""
    }

    private func buildAuthContext(from params: [String: Any], appUrl: String) -> [String: String] {
        [
            AuthContextKeys.clientId: stringParam(params, AuthContextKeys.clientId),
            AuthContextKeys.appId: stringParam(params, AuthContextKeys.appId),
            AuthContextKeys.tabId: stringParam(params, AuthContextKeys.tabId),
            AuthContextKeys.appUrl: appUrl
        ]
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
        shared.sendEvent(withName: UserInfoBridgeContract.eventUserInfoChanged, body: userInfo)
    }
    
    /// 发送登录状态变更事件
    @objc
    static func notifyLoginStatusChanged(_ status: [String: Any]) {
        guard let shared = shared else {
            logger.warning("[UserInfoModule] Module not initialized, cannot send event")
            return
        }
        
        logger.info("[UserInfoModule] Sending onLoginStatusChanged event")
        shared.sendEvent(withName: UserInfoBridgeContract.eventLoginStatusChanged, body: status)
    }
    
    /// 发送用户登出事件
    @objc
    static func notifyUserLogout() {
        guard let shared = shared else {
            logger.warning("[UserInfoModule] Module not initialized, cannot send event")
            return
        }
        
        logger.info("[UserInfoModule] Sending onUserLogout event")
        shared.sendEvent(withName: UserInfoBridgeContract.eventUserLogout, body: nil)
    }

    /// 发送登录结果事件 (wechat code / apple idToken)
    @objc
    static func notifyLoginResult(_ result: [String: Any]) {
        guard let shared = shared else {
            logger.warning("[UserInfoModule] Module not initialized, cannot send event")
            return
        }
        
        logger.info("[UserInfoModule] Sending onLoginResult event")
        shared.sendEvent(withName: LoginResultPayloadContract.eventName, body: result)
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
        
        let appUrl = stringParam(params, AuthContextKeys.appUrl)
        
        // 检查本地用户信息
        if UserInfoManager.shared.getUser() != nil {
            // 有本地用户信息，验证环境是否一致
            if !appUrl.isEmpty {
                let appEnvironment = LoginManager.shared.environmentForUrl(appUrl)
                let currentEnvironment = LoginManager.shared.currentEnvironment
                
                if appEnvironment != currentEnvironment {
                    logger.warning("[UserInfoModule] Environment mismatch: app=\(appEnvironment), current=\(currentEnvironment)")
                    logger.info("[UserInfoModule] Triggering re-login due to environment change")
                    
                    // 构建授权上下文
                    let authContext = buildAuthContext(from: params, appUrl: appUrl)
                    
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
            
            let appId = stringParam(params, AuthContextKeys.appId)
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
                    let authContext = buildAuthContext(from: params, appUrl: appUrl)
                    
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
        let authContext = buildAuthContext(from: params, appUrl: appUrl)
        
        // 设置环境、授权上下文并显示登录弹窗
        DispatchQueue.main.async {
            if !appUrl.isEmpty {
                LoginManager.shared.setupConfig(appUrl)
            }
            // 设置授权上下文，登录成功后会自动进行授权
            LoginManager.shared.setAuthContext(authContext)
        }
    }
    
    /// 处理用户信息请求（提取为独立方法以便复用）
    private func processUserInfoRequest(
        params: [String: Any],
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        let clientId = stringParam(params, AuthContextKeys.clientId)
        let appId = stringParam(params, AuthContextKeys.appId)
        let tabId = stringParam(params, AuthContextKeys.tabId)
        let appUrl = stringParam(params, AuthContextKeys.appUrl)
        
        logger.debug("[UserInfoModule] RN requested current user info")
        logger.debug("[UserInfoModule] Params - clientId: \(clientId), appId: \(appId), tabId: \(tabId), appUrl: \(appUrl)")
        
        if let user = UserInfoManager.shared.getUser() {
            let userDict: [String: Any] = [
                UserInfoBridgeContract.keyId: user.id,
                UserInfoBridgeContract.keyLoginApiUrl: user.loginApiUrl,
                UserInfoBridgeContract.keyUserInfo: user.userInfo,
                UserInfoBridgeContract.keyUserIdentity: user.userIdentity,
                UserInfoBridgeContract.keySessionToken: user.sessionToken,
                UserInfoBridgeContract.keyCreatedAt: user.createdAt.timeIntervalSince1970,
                UserInfoBridgeContract.keyUpdatedAt: user.updatedAt.timeIntervalSince1970,
                // 回传请求参数，方便 RN 端关联
                UserInfoBridgeContract.keyRequestContext: [
                    AuthContextKeys.clientId: clientId,
                    AuthContextKeys.appId: appId,
                    AuthContextKeys.tabId: tabId,
                    AuthContextKeys.appUrl: appUrl
                ]
            ]
            resolve(userDict)
        } else {
            resolve(nil)
        }
    }
}
