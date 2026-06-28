//
//  CanvasTests.swift
//  CanvasTests
//
//  Created by Leo General on 2025/11/14.
//

import Foundation
import Testing
@testable import Canvas

struct CanvasTests {

    @Test func example() async throws {
        // Write your test here and use APIs like `#expect(...)` to check expected conditions.
    }

}

@MainActor
struct DevToolsNetworkTests {
    @Test func appFetchLifecycle_isRecordedInNetworkPanel() async throws {
        let manager = DevToolsManager.shared
        manager.isEnginePaused = false

        let tabId = UUID().uuidString
        let requestId = "req-\(UUID().uuidString)"
        let url = "https://api.example.com/data?x=1"

        manager.logMessage(
            direction: .guestToHost,
            event: "APP_FETCH_REQUEST",
            payload: [
                "requestId": requestId,
                "url": url,
                "options": [
                    "method": "POST",
                    "headers": ["Content-Type": "application/json"],
                    "body": "{\"a\":1}"
                ],
            ],
            tabId: tabId
        )

        try await Task.sleep(nanoseconds: 80_000_000)

        let pending = try #require(manager.networkRequests.first(where: { $0.requestId == requestId }))
        #expect(pending.requestId == requestId)
        #expect(pending.tabId == tabId)
        #expect(pending.url == url)
        #expect(pending.method == "POST")
        #expect(pending.state == .pending)
        #expect(pending.requestHeaders?["Content-Type"] == "application/json")
        #expect((pending.requestBody ?? "").contains("\"a\""))

        manager.logMessage(
            direction: .hostToGuest,
            event: "APP_FETCH_RESPONSE",
            payload: [
                "requestId": requestId,
                "ok": true,
                "status": 200,
                "headers": ["Content-Type": "application/json"],
                "data": ["hello": "world"],
            ],
            tabId: tabId
        )

        try await Task.sleep(nanoseconds: 80_000_000)

        let completed = try #require(manager.networkRequests.first(where: { $0.requestId == requestId }))
        #expect(completed.state == .completed)
        #expect(completed.status == 200)
        #expect(completed.ok == true)
        #expect(completed.responseHeaders?["Content-Type"] == "application/json")
        #expect((completed.responseBody ?? "").contains("\"hello\""))
        #expect(completed.duration != nil)
    }

    @Test func appFetchError_isMarkedFailed() async throws {
        let manager = DevToolsManager.shared
        manager.isEnginePaused = false

        let tabId = UUID().uuidString
        let requestId = "req-\(UUID().uuidString)"

        manager.logMessage(
            direction: .guestToHost,
            event: "APP_FETCH_REQUEST",
            payload: [
                "requestId": requestId,
                "url": "https://api.example.com/fail",
                "options": ["method": "GET"],
            ],
            tabId: tabId
        )

        manager.logMessage(
            direction: .hostToGuest,
            event: "APP_FETCH_RESPONSE",
            payload: [
                "requestId": requestId,
                "ok": false,
                "status": 0,
                "error": "Network error",
            ],
            tabId: tabId
        )

        try await Task.sleep(nanoseconds: 80_000_000)

        let failed = try #require(manager.networkRequests.first(where: { $0.requestId == requestId }))
        #expect(failed.state == .failed)
        #expect(failed.ok == false)
        #expect(failed.error == "Network error")
    }
}
