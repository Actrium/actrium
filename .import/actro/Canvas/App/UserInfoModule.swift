import Foundation
import React

private let logger = AppLogger.reactNative

/// Native module for user info events - Swift → RN EventEmitter
@objc(UserInfoModule)
class UserInfoModule: RCTEventEmitter {
    
    // 单例引用，便于从其他地方发送事件
    private static weak var shared: UserInfoModule?
    
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
    @objc
    func getCurrentUserInfo(_ resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock) {
        logger.debug("[UserInfoModule] RN requested current user info")
        
        if let user = UserInfoManager.shared.getUser() {
            let userDict: [String: Any] = [
                "id": user.id,
                "loginApiUrl": user.loginApiUrl,
                "userInfo": user.userInfo,
                "userIdentity": user.userIdentity,
                "sessionToken": user.sessionToken,
                "createdAt": user.createdAt.timeIntervalSince1970,
                "updatedAt": user.updatedAt.timeIntervalSince1970
            ]
            resolve(userDict)
        } else {
            resolve(nil)
        }
    }
}
