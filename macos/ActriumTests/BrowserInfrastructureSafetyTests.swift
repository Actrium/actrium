import Foundation
import Testing
import ActroCore
@testable import Actro

struct BrowserInfrastructureSafetyTests {
    @Test func sessionRestorationPreservesActiveSourceIndexAfterFiltering() throws {
        let firstID = UUID()
        let activeID = UUID()
        let snapshot = AppleBrowserSessionSnapshot(
            activeTabIndex: 2,
            tabs: [
                Self.sessionTab(id: firstID, title: "First", address: "https://one.example/path"),
                Self.sessionTab(id: UUID(), title: "Invalid", address: "file:///tmp/private"),
                Self.sessionTab(id: activeID, title: "  Active  ", address: "https://two.example")
            ],
            surfaceKind: "macos"
        )

        let restored = try #require(BrowserDataRepository.restoreSession(from: snapshot))

        #expect(restored.tabs.map { $0.id } == [firstID, activeID])
        #expect(restored.currentTabId == activeID)
        #expect(restored.tabs[1].title == "Active")
    }

    @Test func sessionRestorationRejectsMalformedWebAddressesAndDuplicateIDs() throws {
        let duplicateID = UUID()
        let snapshot = AppleBrowserSessionSnapshot(
            activeTabIndex: 0,
            tabs: [
                Self.sessionTab(id: duplicateID, title: "", address: "https://example.com/one"),
                Self.sessionTab(id: duplicateID, title: "Two", address: "https://example.com/two"),
                Self.sessionTab(id: UUID(), title: "Missing host", address: "https:/broken")
            ]
        )

        let restored = try #require(BrowserDataRepository.restoreSession(from: snapshot))

        #expect(restored.tabs.count == 2)
        #expect(Set(restored.tabs.map { $0.id }).count == 2)
        #expect(restored.tabs[0].title == "example.com")
        #expect(restored.currentTabId == duplicateID)
    }

    @Test func emptyOrNonWebSessionFallsBackToFreshStartPage() {
        let snapshot = AppleBrowserSessionSnapshot(
            activeTabIndex: 0,
            tabs: [Self.sessionTab(id: UUID(), title: "Local", address: "actro://settings")]
        )

        #expect(BrowserDataRepository.restoreSession(from: snapshot) == nil)
    }

    @Test func faviconLookupStaysOnTheVisitedOriginAndDropsCredentials() throws {
        let pageURL = try #require(URL(string: "https://user:secret@example.com:8443/a?x=1#part"))
        let faviconURL = try #require(FaviconCache.faviconURL(for: pageURL))

        #expect(faviconURL.absoluteString == "https://example.com:8443/favicon.ico")
        #expect(FaviconCache.cacheKey(for: pageURL) == "https://example.com:8443")
    }

    @Test func faviconPolicyRejectsNonWebSchemes() throws {
        let fileURL = try #require(URL(string: "file:///tmp/page.html"))
        let dataURL = try #require(URL(string: "data:text/html,hello"))

        #expect(FaviconCache.faviconURL(for: fileURL) == nil)
        #expect(FaviconCache.faviconURL(for: dataURL) == nil)
    }

    @Test func credentialOriginsAreSchemeHostAndPortBound() throws {
        let exact = try #require(CredentialOrigin(url: URL(string: "https://EXAMPLE.com/login")!))
        let defaultPort = try #require(CredentialOrigin(url: URL(string: "https://example.com:443/other")!))
        let subdomain = try #require(CredentialOrigin(url: URL(string: "https://accounts.example.com")!))
        let downgraded = try #require(CredentialOrigin(url: URL(string: "http://example.com")!))
        let alternatePort = try #require(CredentialOrigin(url: URL(string: "https://example.com:8443")!))

        #expect(exact == defaultPort)
        #expect(exact != subdomain)
        #expect(exact != downgraded)
        #expect(exact != alternatePort)
    }

    @Test func cancelledAndDownloadTransitionErrorsDoNotReplaceThePage() throws {
        let retryURL = try #require(URL(string: "https://example.com"))
        let cancelled = NSError(domain: NSURLErrorDomain, code: NSURLErrorCancelled)
        let downloadTransition = NSError(domain: "WebKitErrorDomain", code: 102)

        #expect(WebPageFailure.navigation(error: cancelled, retryURL: retryURL) == nil)
        #expect(WebPageFailure.navigation(error: downloadTransition, retryURL: retryURL) == nil)
    }

    @Test func networkAndProcessFailuresRemainRecoverableWithoutRawErrorText() throws {
        let retryURL = try #require(URL(string: "https://example.com"))
        let offline = try #require(WebPageFailure.navigation(
            error: NSError(domain: NSURLErrorDomain, code: NSURLErrorNotConnectedToInternet),
            retryURL: retryURL
        ))
        let terminated = WebPageFailure.processTerminated(retryURL: retryURL)

        #expect(offline.kind == .offline)
        #expect(offline.retryURL == retryURL)
        #expect(terminated.kind == .processTerminated)
        #expect(terminated.retryURL == retryURL)
    }

    private static func sessionTab(
        id: UUID,
        title: String,
        address: String
    ) -> AppleBrowserSessionTab {
        AppleBrowserSessionTab(
            id: id.uuidString,
            title: title,
            address: address,
            createdAtUnixSeconds: 1,
            updatedAtUnixSeconds: 1
        )
    }
}
