//
//  ActriumTests.swift
//  ActriumTests
//
//  Created by Leo General on 2025/11/14.
//

import Foundation
import Testing
@testable import Actrium

struct ActriumTests {

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
        #expect(DevToolsCurlBuilder.build(from: completed)?.contains(url) == true)
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

    @Test func responseOnlyRecordHasNoFabricatedRequestMetadata() async throws {
        let manager = DevToolsManager.shared
        manager.isEnginePaused = false

        let tabId = UUID().uuidString
        let requestId = "response-only-\(UUID().uuidString)"
        manager.logMessage(
            direction: .hostToGuest,
            event: "APP_FETCH_RESPONSE",
            payload: [
                "requestId": requestId,
                "ok": true,
                "status": 201,
                "headers": ["Content-Type": "application/json"],
                "data": ["created": true],
            ],
            tabId: tabId
        )
        try await Task.sleep(nanoseconds: 80_000_000)

        let request = try #require(manager.networkRequests.first { $0.requestId == requestId })
        #expect(request.url == nil)
        #expect(request.method == nil)
        #expect(request.startTime == nil)
        #expect(request.endTime != nil)
        #expect(request.duration == nil)
        #expect(request.state == .completed)
        #expect(request.statusText == "201")
        #expect(request.responseHeaders?["Content-Type"] == "application/json")
        #expect((request.responseBody ?? "").contains("created"))
        #expect(DevToolsCurlBuilder.build(from: request) == nil)

        let sequence = NetworkSequenceViewModel()
        sequence.buildSequence(from: [request])
        #expect(sequence.events.count == 1)
        #expect(sequence.events.first?.type == .response)
        #expect(sequence.participants.contains { !$0.isLocal && $0.name == "Unknown" })
    }

    @Test func responseWithoutStatusIsNotReportedAsPending() async throws {
        let manager = DevToolsManager.shared
        manager.isEnginePaused = false

        let requestId = "missing-request-\(UUID().uuidString)"
        manager.logMessage(
            direction: .hostToGuest,
            event: "APP_FETCH_RESPONSE",
            payload: [
                "requestId": requestId,
                "ok": true,
                "data": "done",
            ],
            tabId: UUID().uuidString
        )
        try await Task.sleep(nanoseconds: 80_000_000)

        let request = try #require(manager.networkRequests.first { $0.requestId == requestId })
        #expect(request.state == .completed)
        #expect(request.status == nil)
        #expect(request.statusText == "N/A")
        #expect(request.startTime == nil)
        #expect(request.method == nil)
        #expect(request.url == nil)
    }

    @Test func lateRequestCompletesOriginalResponseAndSequence() async throws {
        let manager = DevToolsManager.shared
        manager.isEnginePaused = false

        let tabId = UUID().uuidString
        let requestId = "late-request-\(UUID().uuidString)"
        let url = "https://api.example.com/late"
        manager.logMessage(
            direction: .hostToGuest,
            event: "APP_FETCH_RESPONSE",
            payload: [
                "requestId": requestId,
                "ok": true,
                "status": 202,
                "data": ["accepted": true],
            ],
            tabId: tabId
        )
        try await Task.sleep(nanoseconds: 80_000_000)

        let responseOnly = try #require(manager.networkRequests.first { $0.requestId == requestId })
        let originalID = responseOnly.id
        let originalEndTime = responseOnly.endTime
        let originalVersion = manager.networkRequestsVersion
        let responseOnlySequence = NetworkSequenceViewModel()
        responseOnlySequence.buildSequence(from: [responseOnly])
        #expect(responseOnlySequence.events.count == 1)

        manager.logMessage(
            direction: .guestToHost,
            event: "APP_FETCH_REQUEST",
            payload: [
                "requestId": requestId,
                "url": url,
                "options": [
                    "method": "PUT",
                    "headers": ["Content-Type": "text/plain"],
                    "body": "payload",
                ],
            ],
            tabId: tabId
        )
        try await Task.sleep(nanoseconds: 80_000_000)

        let matchingRequests = manager.networkRequests.filter { $0.requestId == requestId }
        #expect(matchingRequests.count == 1)
        let completed = try #require(matchingRequests.first)
        #expect(completed.id == originalID)
        #expect(completed.url == url)
        #expect(completed.method == "PUT")
        #expect(completed.startTime != nil)
        #expect(completed.endTime == originalEndTime)
        #expect(completed.duration == nil)
        #expect(completed.status == 202)
        #expect(completed.state == .completed)
        #expect((completed.responseBody ?? "").contains("accepted"))
        #expect(manager.networkRequestsVersion > originalVersion)
        #expect(DevToolsCurlBuilder.build(from: completed)?.contains("PUT") == true)

        let completedSequence = NetworkSequenceViewModel()
        completedSequence.buildSequence(from: [completed])
        #expect(completedSequence.events.filter { $0.type == .request }.count == 1)
        #expect(completedSequence.events.filter { $0.type == .response }.count == 1)
    }
}
