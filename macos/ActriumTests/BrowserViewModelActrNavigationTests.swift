import Foundation
import os
import Testing
@testable import Actrium

/// Regression coverage for review feedback on PR #182 (P2): an actr://
/// resolution completing after the user already navigated the same tab to a
/// different page must not overwrite the newer navigation — neither by
/// loading the stale package nor by writing the error placeholder title.
@MainActor
@Suite(.serialized)
final class BrowserViewModelActrNavigationTests {
    /// os_unfair_lock is a C API without Swift's "unavailable from async contexts"
    /// annotation, keeping `withCheckedThrowingContinuation` closures warning-free.
    private struct AsyncSafeLock {
        private let rawLock: UnsafeMutablePointer<os_unfair_lock>

        init() {
            rawLock = .allocate(capacity: 1)
            rawLock.initialize(to: os_unfair_lock())
        }

        func locking<T>(_ body: () throws -> T) rethrows -> T {
            os_unfair_lock_lock(rawLock)
            defer { os_unfair_lock_unlock(rawLock) }
            return try body()
        }
    }

    /// Captures the in-flight `resolveActorPackage` call so the test decides
    /// exactly when resolution finishes, then counts completions so the test
    /// can wait for the resolution task queue to drain.
    private final class PendingResolver: @unchecked Sendable {
        private let lock = AsyncSafeLock()
        private var continuations: [CheckedContinuation<URL, any Error>] = []
        private var invocations = 0
        private var completions = 0

        func makeResolver() -> (URL) async throws -> URL {
            { _ in
                defer { self.lock.locking { self.completions += 1 } }
                return try await withCheckedThrowingContinuation { continuation in
                    self.lock.locking {
                        self.invocations += 1
                        self.continuations.append(continuation)
                    }
                }
            }
        }

        func waitForInvocation(timeoutSeconds: TimeInterval = 5) async {
            let deadline = Date().addingTimeInterval(timeoutSeconds)
            while Date() < deadline {
                try? await Task.sleep(nanoseconds: 10_000_000)
                let started = lock.locking { invocations > 0 }
                if started { return }
            }
            Issue.record("Timed out waiting for the actr package resolver to start")
        }

        func complete(with result: Result<URL, ActrAuthorityError>) {
            let captured = lock.locking { () -> [CheckedContinuation<URL, any Error>] in
                let all = continuations
                continuations.removeAll()
                return all
            }
            guard let continuation = captured.first else {
                Issue.record("Expected one in-flight actr resolution")
                return
            }
            switch result {
            case .success(let url):
                continuation.resume(returning: url)
            case .failure(let error):
                continuation.resume(throwing: error)
            }
        }

        /// Waits until the resolver finished (both values were delivered) and gives
        /// the resolution task scheduling room to run its guarded continuation.
        func awaitCompletionAndTaskDrain() async {
            let deadline = Date().addingTimeInterval(5)
            while Date() < deadline {
                let done = lock.locking { completions > 0 }
                if done { break }
                try? await Task.sleep(nanoseconds: 10_000_000)
            }
            try? await Task.sleep(nanoseconds: 100_000_000)
        }
    }

    private func makeViewModelWithPendingResolver() -> (BrowserViewModel, PendingResolver, UUID) {
        let resolver = PendingResolver()
        let viewModel = BrowserViewModel(resolveActorPackage: resolver.makeResolver())
        let tabId = viewModel.currentTabId ?? UUID()
        return (viewModel, resolver, tabId)
    }

    @Test func staleSuccessDoesNotOverwriteNewerNavigation() async throws {
        let (viewModel, resolver, tabId) = makeViewModelWithPendingResolver()
        try #require(viewModel.tabs.contains { $0.id == tabId })

        viewModel.navigateCurrentTab(to: "actr://localhost:8081/echo")
        #expect(viewModel.tabs.first { $0.id == tabId }?.title == "Loading Actor")
        await resolver.waitForInvocation()

        // User navigates the same tab elsewhere before the download finishes.
        viewModel.navigateCurrentTab(to: "https://example.com")
        resolver.complete(with: .success(URL(fileURLWithPath: "/tmp/stale-package.actr")))
        await resolver.awaitCompletionAndTaskDrain()

        let tab = try #require(viewModel.tabs.first { $0.id == tabId })
        #expect(tab.url == URL(string: "https://example.com"))
        #expect(tab.title == "example.com")
        #expect(tab.appId == nil)
    }

    @Test func staleFailureDoesNotOverwriteNewerNavigation() async throws {
        let (viewModel, resolver, tabId) = makeViewModelWithPendingResolver()
        try #require(viewModel.tabs.contains { $0.id == tabId })

        viewModel.navigateCurrentTab(to: "actr://localhost:8081/echo")
        await resolver.waitForInvocation()

        viewModel.navigateCurrentTab(to: "https://example.com")
        resolver.complete(with: .failure(.packageDigestMismatch))
        await resolver.awaitCompletionAndTaskDrain()

        let tab = try #require(viewModel.tabs.first { $0.id == tabId })
        #expect(tab.url == URL(string: "https://example.com"))
        #expect(tab.title == "example.com")
    }
}
