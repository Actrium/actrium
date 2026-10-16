import Foundation
import ActriumCore

// MARK: - Login / Auth Bridge

enum LoginResultPayloadContract {
    static let eventName = runtimeLoginResultEventName()
    static let type = runtimeLoginResultTypeKey()
    static let code = runtimeLoginResultCodeKey()
    static let idToken = runtimeLoginResultIdTokenKey()
    static let error = runtimeLoginResultErrorKey()
    static let message = runtimeLoginResultMessageKey()
    static let appleType = runtimeLoginTypeApple()
    static let wechatType = runtimeLoginTypeWechat()
}

enum AuthContextKeys {
    static let clientId = runtimeAuthContextClientIdKey()
    static let appId = runtimeAuthContextAppIdKey()
    static let tabId = runtimeAuthContextTabIdKey()
    static let appUrl = runtimeAuthContextAppUrlKey()
}

enum AuthEnvironmentContract {
    static let dev = authEnvironmentDev()
    static let beta = authEnvironmentBeta()
    static let prod = authEnvironmentProd()
}

enum AuthCoreBridge {
    static func environment(from url: String) -> String {
        authEnvironmentFromUrl(url: url)
    }

    static func environment(fromAppUrl url: String) -> String {
        authEnvironmentFromAppUrl(url: url)
    }

    static func storageSuffix(for environment: String) -> String {
        authEnvironmentStorageSuffix(environment: environment)
    }

    static func accountBaseURL(for environment: String) -> String {
        authAccountBaseUrl(environment: environment)
    }

    static func accessControlURL(appUrl: String, appId: String, userId: String) -> String? {
        authAccessControlUrlFromAppUrl(appUrl: appUrl, appId: appId, userId: userId)
    }

    static func isAuthorizedCode(_ code: Int) -> Bool {
        Int32(exactly: code).map { authIsAuthorizedCode(code: $0) } ?? false
    }
}

// MARK: - User Info Bridge

enum UserInfoBridgeContract {
    private static let value = userInfoBridgeContract()

    static let eventUserInfoChanged = value.eventUserInfoChanged
    static let eventLoginStatusChanged = value.eventLoginStatusChanged
    static let eventUserLogout = value.eventUserLogout
    static let keyId = value.keyId
    static let keyLoginApiUrl = value.keyLoginApiUrl
    static let keyUserInfo = value.keyUserInfo
    static let keyUserIdentity = value.keyUserIdentity
    static let keySessionToken = value.keySessionToken
    static let keyCreatedAt = value.keyCreatedAt
    static let keyUpdatedAt = value.keyUpdatedAt
    static let keyRequestContext = value.keyRequestContext
    static let keyIsLoggedIn = value.keyIsLoggedIn
}

// MARK: - TTS Bridge

enum TTSBridgeContract {
    private static let value = ttsBridgeContract()

    static let eventStart = value.eventStart
    static let eventPause = value.eventPause
    static let eventResume = value.eventResume
    static let eventFinish = value.eventFinish
    static let eventError = value.eventError
    static let keyRate = value.keyRate
    static let keyPitchMultiplier = value.keyPitchMultiplier
    static let keyVolume = value.keyVolume
    static let keyLanguage = value.keyLanguage
    static let keyVoiceIdentifier = value.keyVoiceIdentifier
    static let keyCancelled = value.keyCancelled
    static let keyMessage = value.keyMessage
}
