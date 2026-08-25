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

    @Test func provisionalNavigationSuppressesNonAuthoritativeURLReads() {
        var gate = WebNavigationURLObservationGate()

        gate.navigationStarted()

        #expect(gate.makeToken() == nil)
        let committed = gate.makeToken(allowsDuringNavigation: true)
        #expect(committed != nil)
        #expect(committed.map(gate.accepts) == true)
    }

    @Test func newerObservationRejectsStaleCompletion() throws {
        var gate = WebNavigationURLObservationGate()
        let olderCandidate = gate.makeToken()
        let newerCandidate = gate.makeToken()
        let older = try #require(olderCandidate)
        let newer = try #require(newerCandidate)

        #expect(!gate.accepts(older))
        #expect(gate.accepts(newer))
    }

    @Test func finishedNavigationResultSurvivesSameDocumentKVOButNotANewNavigation() throws {
        var gate = WebNavigationURLObservationGate()
        gate.navigationStarted()
        gate.navigationEnded()
        let finishedCandidate = gate.makeToken(authoritative: true)
        let finished = try #require(finishedCandidate)
        _ = gate.makeToken()

        #expect(gate.accepts(finished))

        gate.navigationStarted()
        #expect(!gate.accepts(finished))
    }

    @Test func rapidAutomaticTopLevelNavigationOpensCircuitAndRequiresQuietPeriod() {
        var guardState = RapidTopLevelNavigationGuard(
            window: 2,
            maximumAutomaticNavigations: 3,
            quietPeriod: 2
        )

        let first = guardState.shouldAllowAutomaticNavigation(at: 0.0)
        let second = guardState.shouldAllowAutomaticNavigation(at: 0.1)
        let third = guardState.shouldAllowAutomaticNavigation(at: 0.2)
        let opensCircuit = guardState.shouldAllowAutomaticNavigation(at: 0.3)
        let remainsBlocked = guardState.shouldAllowAutomaticNavigation(at: 1.0)
        let retryExtendsQuietPeriod = guardState.shouldAllowAutomaticNavigation(at: 2.5)
        let resumesAfterQuietPeriod = guardState.shouldAllowAutomaticNavigation(at: 4.6)

        #expect(first)
        #expect(second)
        #expect(third)
        #expect(!opensCircuit)
        #expect(!remainsBlocked)
        #expect(!retryExtendsQuietPeriod)
        #expect(resumesAfterQuietPeriod)
    }

    @Test func explicitNavigationResetsRapidNavigationCircuit() {
        var guardState = RapidTopLevelNavigationGuard(
            window: 2,
            maximumAutomaticNavigations: 1,
            quietPeriod: 10
        )

        let first = guardState.shouldAllowAutomaticNavigation(at: 0.0)
        let opensCircuit = guardState.shouldAllowAutomaticNavigation(at: 0.1)

        guardState.registerExplicitNavigation()

        let afterReset = guardState.shouldAllowAutomaticNavigation(at: 0.2)

        #expect(first)
        #expect(!opensCircuit)
        #expect(afterReset)
    }
}
