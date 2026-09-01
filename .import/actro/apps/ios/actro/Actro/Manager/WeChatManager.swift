import UIKit

// 微信回调结果通知名称
extension Notification.Name {
    static let wechatLoginResult = Notification.Name("wechatLoginResult")
    static let wechatShareResult = Notification.Name("wechatShareResult")
}

class WeChatManager: NSObject, WXApiDelegate {
    static let shared = WeChatManager()
    
    // 初始化微信 SDK（在 App 启动时调用）
    func registerApp() {
        WXApi.registerApp(AppConstants.weChatAppId, universalLink: AppConstants.universalLink) // 需在微信开放平台配置
    }
    
    // 处理微信回调的 URL（从 AppDelegate 或 SceneDelegate 转发）
    func handleOpenURL(_ url: URL) -> Bool {
        return WXApi.handleOpen(url, delegate: self)
    }
    
    private let logger = AppLogger.app

    // 微信登录
    func sendLoginRequest() {
        if !WXApi.isWXAppInstalled() {
            logger.error("[WeChatManager] 微信未安装，无法进行登录跳转")
            let result: [String: Any] = [
                "success": false,
                "errorMsg": "未安装微信"
            ]
            NotificationCenter.default.post(name: .wechatLoginResult, object: result)
            return
        }
        
        let req = SendAuthReq()
        req.scope = "snsapi_userinfo"
        req.state = "123"
        WXApi.send(req)
    }
    
    // 微信分享（示例：分享文本）
    func shareText(content: String) {
        let message = WXMediaMessage()
        message.description = content
        
        let req = SendMessageToWXReq()
        req.message = message
        req.scene = Int32(WXSceneSession.rawValue) // 分享到会话（WXSceneTimeline 是朋友圈）
        WXApi.send(req)
    }
    
    // MARK: - WXApiDelegate（微信回调）
    func onResp(_ resp: BaseResp) {
        if let authResp = resp as? SendAuthResp {
            // 登录回调
            let result: [String: Any] = [
                LoginResultPayloadContract.code: authResp.code ?? "",
                "errorCode": authResp.errCode,
                "errorMsg": authResp.errStr
            ]
            NotificationCenter.default.post(name: .wechatLoginResult, object: result)
        } else if let shareResp = resp as? SendMessageToWXResp {
            // 分享回调
            let result: [String: Any] = [
                "errorCode": shareResp.errCode,
                "errorMsg": shareResp.errStr
            ]
            NotificationCenter.default.post(name: .wechatShareResult, object: result)
        }
    }
    
    func onReq(_ req: BaseReq) {} // 暂不需要处理
}
