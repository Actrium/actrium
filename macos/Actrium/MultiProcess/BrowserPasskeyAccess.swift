import AuthenticationServices
import Foundation

final class BrowserPasskeyAccess {
    static let shared = BrowserPasskeyAccess()

    private var authorizationRequester: AnyObject?

    private init() {}

    func prepareForWebBrowsing() {
        guard #available(macOS 13.3, *) else {
            NSLog("BrowserPasskeyAccess: platform passkeys require macOS 13.3 or newer")
            return
        }

        let requester: BrowserPasskeyAuthorizationRequester
        if let existingRequester = authorizationRequester as? BrowserPasskeyAuthorizationRequester {
            requester = existingRequester
        } else {
            let newRequester = BrowserPasskeyAuthorizationRequester()
            authorizationRequester = newRequester
            requester = newRequester
        }

        requester.prepareForWebBrowsing()
    }

    func authorizationStateForDiagnostics() -> String {
        guard #available(macOS 13.3, *) else {
            return "unavailable"
        }

        let requester: BrowserPasskeyAuthorizationRequester
        if let existingRequester = authorizationRequester as? BrowserPasskeyAuthorizationRequester {
            requester = existingRequester
        } else {
            let newRequester = BrowserPasskeyAuthorizationRequester()
            authorizationRequester = newRequester
            requester = newRequester
        }

        return requester.authorizationStateForDiagnostics()
    }
}

@available(macOS 13.3, *)
private final class BrowserPasskeyAuthorizationRequester {
    private let manager = ASAuthorizationWebBrowserPublicKeyCredentialManager()
    private var didRequestAuthorization = false

    func prepareForWebBrowsing() {
        switch manager.authorizationStateForPlatformCredentials {
        case .authorized:
            NSLog("BrowserPasskeyAccess: platform passkeys are authorized")
        case .denied:
            NSLog("BrowserPasskeyAccess: platform passkeys are denied for this browser")
        case .notDetermined:
            requestAuthorizationIfNeeded()
        @unknown default:
            NSLog("BrowserPasskeyAccess: unknown platform passkey authorization state")
        }
    }

    func authorizationStateForDiagnostics() -> String {
        describe(manager.authorizationStateForPlatformCredentials)
    }

    private func requestAuthorizationIfNeeded() {
        guard !didRequestAuthorization else {
            return
        }

        didRequestAuthorization = true
        manager.requestAuthorizationForPublicKeyCredentials { [weak self] authorizationState in
            guard let self else {
                return
            }
            NSLog(
                "BrowserPasskeyAccess: platform passkey authorization result=%@",
                self.describe(authorizationState)
            )
        }
    }

    private func describe(
        _ state: ASAuthorizationWebBrowserPublicKeyCredentialManager.AuthorizationState
    ) -> String {
        switch state {
        case .authorized:
            return "authorized"
        case .denied:
            return "denied"
        case .notDetermined:
            return "notDetermined"
        @unknown default:
            return "unknown"
        }
    }
}
