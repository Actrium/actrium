import Foundation
import React

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
        
        // 监听登录成功事件
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleLoginSuccess),
            name: NSNotification.Name("LoginSuccessNotification"),
            object: nil
        )
    }
    
    deinit {
        NotificationCenter.default.removeObserver(self)
        logger.notice("[UserInfoModule] Deinitialized")
    }
    
    /// 处理登录成功通知，解析所有等待的请求
    @objc private func handleLoginSuccess() {
        logger.info("[UserInfoModule] Login success, processing \(self.pendingRequests.count) pending requests")
        
        guard !pendingRequests.isEmpty else { return }
        
        // 处理所有等待的请求
        let requests = pendingRequests
        pendingRequests.removeAll()
        
        for request in requests {
            processUserInfoRequest(
                params: request.params,
                resolve: request.resolve,
                reject: request.reject
            )
        }
    }
    
    // MARK: - RCTEventEmitter
    
    /// 声明支持的事件列表
    override func supportedEvents() -> [String]! {
        return [
            "onUserInfoChanged",
            "onLoginStatusChanged",
            "onUserLogout"
        ]
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
        
        // 先尝试直接获取用户信息
        if let user = UserInfoManager.shared.getUser() {
            logger.debug("[UserInfoModule] Found local user info, returning immediately")
            processUserInfoRequest(params: params, resolve: resolve, reject: reject)
            return
        }
        
        // 检查是否已登录（LoginManager 中的全局用户）
        if LoginManager.shared.isLoggedIn, let currentUser = LoginManager.shared.currentUser {
            logger.debug("[UserInfoModule] User is logged in globally, saving to local storage")
            // 将全局用户信息保存到当前 appId 的本地存储
            // TODO: 需要实现将 currentUser 转换并保存到 UserInfoManager
            // 暂时先返回错误，触发登录授权流程
        }
        
        // 用户信息为空，添加到等待队列并触发登录
        logger.info("[UserInfoModule] No user info found, triggering login authorization")
        pendingRequests.append((params: params, resolve: resolve, reject: reject))
        
        // 在主线程显示登录弹窗
        DispatchQueue.main.async {
            LoginManager.shared.showLogin(.loginAuth)
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
