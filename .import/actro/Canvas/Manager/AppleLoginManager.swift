import UIKit
import AuthenticationServices

// Apple 登录回调结果通知名称
extension Notification.Name {
    static let appleLoginResult = Notification.Name("appleLoginResult")
}

class AppleLoginManager: NSObject {
    static let shared = AppleLoginManager()
    
    private override init() {
        super.init()
    }
    
    // Apple 登录
    func signInWithApple() {
        let request = ASAuthorizationAppleIDProvider().createRequest()
        request.requestedScopes = [.fullName, .email]
        
        let authorizationController = ASAuthorizationController(authorizationRequests: [request])
        authorizationController.delegate = self
        authorizationController.performRequests()
    }
    
    // 检查 Apple ID 认证状态
    func checkAppleIDCredentialState(userID: String, completion: @escaping (ASAuthorizationAppleIDProvider.CredentialState) -> Void) {
        let provider = ASAuthorizationAppleIDProvider()
        provider.getCredentialState(forUserID: userID) { credentialState, error in
            DispatchQueue.main.async {
                completion(credentialState)
            }
        }
    }
}

// MARK: - ASAuthorizationControllerDelegate
extension AppleLoginManager: ASAuthorizationControllerDelegate {
    func authorizationController(controller: ASAuthorizationController, didCompleteWithAuthorization authorization: ASAuthorization) {
        if let appleIDCredential = authorization.credential as? ASAuthorizationAppleIDCredential {
            let userID = appleIDCredential.user
            let email = appleIDCredential.email
            let fullName = appleIDCredential.fullName
            let identityToken = appleIDCredential.identityToken
            let authorizationCode = appleIDCredential.authorizationCode
            
            // 构建结果数据
            var result: [String: Any] = [
                "userID": userID,
                "success": true
            ]
            
            if let email = email {
                result["email"] = email
            }
            
            if let fullName = fullName {
                var nameComponents: [String: String] = [:]
                if let givenName = fullName.givenName {
                    nameComponents["givenName"] = givenName
                }
                if let familyName = fullName.familyName {
                    nameComponents["familyName"] = familyName
                }
                result["fullName"] = nameComponents
            }
            
            if let identityToken = identityToken,
               let tokenString = String(data: identityToken, encoding: .utf8) {
                result["identityToken"] = tokenString
            }
            
            if let authorizationCode = authorizationCode,
               let codeString = String(data: authorizationCode, encoding: .utf8) {
                result["authorizationCode"] = codeString
            }
            
            // 发送通知
            NotificationCenter.default.post(name: .appleLoginResult, object: result)
        }
    }
    
    func authorizationController(controller: ASAuthorizationController, didCompleteWithError error: Error) {
        let result: [String: Any] = [
            "success": false,
            "errorMsg": error.localizedDescription,
            "error": error
        ]
        
        NotificationCenter.default.post(name: .appleLoginResult, object: result)
    }
}
