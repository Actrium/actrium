import AuthenticationServices
import Foundation
import Security

final class BrowserPasskeyAccess {
    static let shared = BrowserPasskeyAccess()

    private var authorizationRequester: AnyObject?

    private init() {}

    func prepareForWebBrowsing() {
        guard #available(macOS 13.3, *) else {
            NSLog("BrowserPasskeyAccess: platform passkeys require macOS 13.3 or newer")
            return
        }

        BrowserPasskeyEntitlementDiagnostics.logCurrentProcessEntitlements()

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

private enum BrowserPasskeyEntitlementDiagnostics {
    private static let browserEntitlement = "com.apple.developer.web-browser"
    private static let passkeyEntitlement = "com.apple.developer.web-browser.public-key-credential"

    static func logCurrentProcessEntitlements() {
        let hasBrowserEntitlement = hasBooleanEntitlement(browserEntitlement)
        let hasPasskeyEntitlement = hasBooleanEntitlement(passkeyEntitlement)

        guard hasBrowserEntitlement && hasPasskeyEntitlement else {
            NSLog(
                "BrowserPasskeyAccess: signed app is missing browser passkey entitlements (web-browser=%@, public-key-credential=%@)",
                hasBrowserEntitlement.description,
                hasPasskeyEntitlement.description
            )
            return
        }

        NSLog("BrowserPasskeyAccess: signed app contains browser passkey entitlements")
    }

    private static func hasBooleanEntitlement(_ key: String) -> Bool {
        guard let task = SecTaskCreateFromSelf(nil),
              let value = SecTaskCopyValueForEntitlement(task, key as CFString, nil) else {
            return false
        }

        return (value as? Bool) == true
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
