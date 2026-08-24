import Foundation
import Testing
@testable import Actro

struct WebNavigationLoadStateTests {
    @Test func firstAttachmentLoadsExactlyOnce() throws {
        var state = WebNavigationLoadState()
        let url = try #require(URL(string: "https://example.com"))

        #expect(state.initialRequest(for: url) == url)
        #expect(state.initialRequest(for: url) == nil)
    }

    @Test func switchingAwayAndBackNeverCreatesANavigation() throws {
        var state = WebNavigationLoadState()
        let url = try #require(URL(string: "https://example.com/page"))

        _ = state.initialRequest(for: url)
        #expect(state.initialRequest(for: url) == nil)
    }

    @Test func observedRedirectDoesNotBounceBackIntoWebView() throws {
        var state = WebNavigationLoadState()
        let requestedURL = try #require(URL(string: "https://example.com"))
        let redirectedURL = try #require(URL(string: "https://www.example.com/home"))

        _ = state.initialRequest(for: requestedURL)
        state.observeWebViewURL(redirectedURL)

        #expect(state.navigationRequest(for: redirectedURL) == nil)
        #expect(state.initialRequest(for: redirectedURL) == nil)
    }

    @Test func explicitAddressChangeStillNavigates() throws {
        var state = WebNavigationLoadState()
        let firstURL = try #require(URL(string: "https://example.com/one"))
        let secondURL = try #require(URL(string: "https://example.com/two"))

        _ = state.initialRequest(for: firstURL)

        #expect(state.navigationRequest(for: secondURL) == secondURL)
        #expect(state.navigationRequest(for: secondURL) == nil)
    }

    @Test func invalidatedTabRejectsEveryLateNavigationEvent() throws {
        var state = WebNavigationLoadState()
        let firstURL = try #require(URL(string: "https://example.com/one"))
        let lateURL = try #require(URL(string: "https://example.com/late"))

        _ = state.initialRequest(for: firstURL)
        state.invalidate()

        #expect(state.isInvalidated)
        #expect(state.initialRequest(for: lateURL) == nil)
        #expect(state.navigationRequest(for: lateURL) == nil)
        #expect(state.observeWebViewURL(lateURL) == false)
        #expect(state.currentURL == firstURL)
    }
}
