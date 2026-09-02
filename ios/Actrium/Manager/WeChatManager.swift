import UIKit

// Message Revert Result Notification Name
extension Notification.Name {
    static let wechatLoginResult = Notification.Name("wechatLoginResult")
    static let wechatShareResult = Notification.Name("wechatShareResult")
}

class WeChatManager: NSObject, WXApiDelegate {
    static let shared = WeChatManager()
    
    // Initialise Micro-Mercure SDK(In the middle of the day) App Call on startup)
    func registerApp() {
        WXApi.registerApp(AppConstants.weChatAppId, universalLink: AppConstants.universalLink) // Need to configure on the Open-Winded Platform
    }
    
    // We're processing the tweets. URLFrom AppDelegate or SceneDelegate Forward)
    func handleOpenURL(_ url: URL) -> Bool {
        return WXApi.handleOpen(url, delegate: self)
    }
    
    private let logger = AppLogger.app

    // Micro-Mercure Login
    func sendLoginRequest() {
        if !WXApi.isWXAppInstalled() {
            logger.error("[WeChatManager] Weavers are not installed, so login jump is not possible")
            let result: [String: Any] = [
                "success": false,
                "errorMsg": "No Twitter installed"
            ]
            NotificationCenter.default.post(name: .wechatLoginResult, object: result)
            return
        }
        
        let req = SendAuthReq()
        req.scope = "snsapi_userinfo"
        req.state = "123"
        WXApi.send(req)
    }
    
    // Micro-credit sharing (example: shared text)
    func shareText(content: String) {
        let message = WXMediaMessage()
        message.description = content
        
        let req = SendMessageToWXReq()
        req.message = message
        req.scene = Int32(WXSceneSession.rawValue) // Share to Session (Present)WXSceneTimeline It's a circle of friends.
        WXApi.send(req)
    }
    
    // MARK: - WXApiDelegate(Whispering echoes)
    func onResp(_ resp: BaseResp) {
        if let authResp = resp as? SendAuthResp {
            // Login Retrieval
            let result: [String: Any] = [
                LoginResultPayloadContract.code: authResp.code ?? "",
                "errorCode": authResp.errCode,
                "errorMsg": authResp.errStr
            ]
            NotificationCenter.default.post(name: .wechatLoginResult, object: result)
        } else if let shareResp = resp as? SendMessageToWXResp {
            // Share Back
            let result: [String: Any] = [
                "errorCode": shareResp.errCode,
                "errorMsg": shareResp.errStr
            ]
            NotificationCenter.default.post(name: .wechatShareResult, object: result)
        }
    }
    
    func onReq(_ req: BaseReq) {} // It's not going to be taken care of.
}
