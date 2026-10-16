import Foundation
import React
import A1SDK

private let logger = AppLogger.reactNative

/// Native module for user info events - Swift → RN EventEmitter
@objc(UserInfoModule)
class UserInfoModule: RCTEventEmitter {
    
    // Single case to facilitate the sending of events from elsewhere
    private static weak var shared: UserInfoModule?
    
    // Request queue pending login
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
    
    /// List of events declared to support
    override func supportedEvents() -> [String]! {
        return [
            UserInfoBridgeContract.eventUserInfoChanged,
            UserInfoBridgeContract.eventLoginStatusChanged,
            UserInfoBridgeContract.eventUserLogout,
            LoginResultPayloadContract.eventName
        ]
    }
    
    /// Trigger Login
    @objc
    func loginWithType(_ type: String) {
        let loginType: LoginType = type == LoginResultPayloadContract.wechatType ? .wechat : .apple
        DispatchQueue.main.async {
            LoginManager.shared.loginWithType(loginType)
        }
    }
    
    /// Make sure the main queue is initialised
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
    
    // MARK: - Public Static Methods (Calling for dispatch from outside)
    
    /// Send user information change events
    @objc
    static func notifyUserInfoChanged(_ userInfo: [String: Any]) {
        guard let shared = shared else {
            logger.warning("[UserInfoModule] Module not initialized, cannot send event")
            return
        }
        
        logger.info("[UserInfoModule] Sending onUserInfoChanged event")
        shared.sendEvent(withName: UserInfoBridgeContract.eventUserInfoChanged, body: userInfo)
    }
    
    /// Sending login status change event
    @objc
    static func notifyLoginStatusChanged(_ status: [String: Any]) {
        guard let shared = shared else {
            logger.warning("[UserInfoModule] Module not initialized, cannot send event")
            return
        }
        
        logger.info("[UserInfoModule] Sending onLoginStatusChanged event")
        shared.sendEvent(withName: UserInfoBridgeContract.eventLoginStatusChanged, body: status)
    }
    
    /// Send user logout event
    @objc
    static func notifyUserLogout() {
        guard let shared = shared else {
            logger.warning("[UserInfoModule] Module not initialized, cannot send event")
            return
        }
        
        logger.info("[UserInfoModule] Sending onUserLogout event")
        shared.sendEvent(withName: UserInfoBridgeContract.eventUserLogout, body: nil)
    }

    /// Send login result events (wechat code / apple idToken)
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
    
    /// RN Actively request current user information (optional)
    /// - Parameters:
    ///   - params: Organisation clientId, appId, tabId, appUrl The numeric dictionary
    @objc
    func getCurrentUserInfo(
        _ params: [String: Any],
        resolver resolve: @escaping RCTPromiseResolveBlock,
        rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        logger.debug("[UserInfoModule] RN requested current user info")
        logger.debug("[UserInfoModule] Params: \(params)")
        
        let appUrl = stringParam(params, AuthContextKeys.appUrl)
        
        // Check local user information
        if UserInfoManager.shared.getUser() != nil {
            // Local user information to verify the consistency of the environment
            if !appUrl.isEmpty {
                let appEnvironment = LoginManager.shared.environmentForUrl(appUrl)
                let currentEnvironment = LoginManager.shared.currentEnvironment
                
                if appEnvironment != currentEnvironment {
                    logger.warning("[UserInfoModule] Environment mismatch: app=\(appEnvironment), current=\(currentEnvironment)")
                    logger.info("[UserInfoModule] Triggering re-login due to environment change")
                    
                    // Build Mandate Context
                    let authContext = buildAuthContext(from: params, appUrl: appUrl)
                    
                    // Unsatisfactory environment. Trigger re-entry
                    pendingRequests.append((params: params, resolve: resolve, reject: reject))
                    DispatchQueue.main.async {
                        LoginManager.shared.setupConfig(appUrl)
                        LoginManager.shared.setAuthContext(authContext)
                    }
                    return
                }
            }
            
            // Consistent environment, to verify whether user authorization is available
            logger.debug("[UserInfoModule] Found local user info with matching environment, checking authorization")
            
            let appId = stringParam(params, AuthContextKeys.appId)
            // Fetch from local user information userId
            guard let user = UserInfoManager.shared.getUser() else {
                logger.error("[UserInfoModule] Failed to get user info for authorization check")
                processUserInfoRequest(params: params, resolve: resolve, reject: reject)
                return
            }
            
            // Parsing userIdentity Access userId(Assumptions) userIdentity Yeah. user.cn)
            let userId = user.userIdentity
            
            // Authenticate user authorization
            LoginManager.shared.checkUserAuthorization(appId: appId, userId: userId, appUrl: appUrl) { [weak self] isAuthorized in
                guard let self = self else { return }
                
                if isAuthorized {
                    // Authorized to return user information directly
                    logger.debug("[UserInfoModule] User is authorized, returning user info")
                    self.processUserInfoRequest(params: params, resolve: resolve, reject: reject)
                } else {
                    // Unauthorized, triggers the delegation process
                    logger.info("[UserInfoModule] User not authorized, triggering authorization flow")
                    
                    // Build Mandate Context
                    let authContext = buildAuthContext(from: params, appUrl: appUrl)
                    
                    // Request to be joined in waiting queue
                    self.pendingRequests.append((params: params, resolve: resolve, reject: reject))
                    
                    // Trigger authorized bullet window
                    DispatchQueue.main.async {
                        LoginManager.shared.setAuthContext(authContext)
                    }
                }
            }
            return
        }
        
        // No local user information, no login, trigger login process
        // Automatically authorize and save to local after login, without a bug-in reminder box
        logger.info("[UserInfoModule] No local user info, user not logged in, triggering login flow")
        pendingRequests.append((params: params, resolve: resolve, reject: reject))
        
        // Build Mandate Context
        let authContext = buildAuthContext(from: params, appUrl: appUrl)
        
        // Set environment, authorize context and display login window
        DispatchQueue.main.async {
            if !appUrl.isEmpty {
                LoginManager.shared.setupConfig(appUrl)
            }
            // Sets the context of the authorization, which automatically authorizes the login after success
            LoginManager.shared.setAuthContext(authContext)
        }
    }
    
    /// Processing of user information requests (extracted as a stand-alone method for reuse)
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
                // Remittance request parameters, easy RN End-Contact
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
