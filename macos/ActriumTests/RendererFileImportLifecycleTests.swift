import ActriumIPC
import AppKit
import Foundation
import Testing
@testable import Actrium

@MainActor
struct RendererFileImportLifecycleTests {
    @Test func reloadCancelsTheOldRequestAndIgnoresItsLateResult() async throws {
        let window = PickerTestWindow(contentRect: .zero, styleMask: [], backing: .buffered, defer: true)
        let oldPicker = ControlledPicker()
        let newPicker = ControlledPicker()
        var pickers = [oldPicker, newPicker]
        let broker = RendererFileImport(makePicker: { pickers.removeFirst() })
        let tabId = UUID()
        defer {
            broker.cancel(tabId: tabId)
            oldPicker.finish(.success([]))
            newPicker.finish(.success([]))
        }
        let oldRequest = Task { try await request(broker, tabId: tabId, reloadKey: 1) { window } }
        try await oldPicker.waitUntilStarted()
        broker.update(tabId: tabId, reloadKey: 2)
        #expect(try await oldRequest.value.error != nil)
        #expect(oldPicker.cancelled)

        let newRequest = Task { try await request(broker, tabId: tabId, reloadKey: 2) { window } }
        try await newPicker.waitUntilStarted()
        oldPicker.finish(.success([.init(name: "stale.txt", data: Data([1]))]))
        await Task.yield()
        let busy = try await request(broker, tabId: tabId, reloadKey: 2) { window }
        #expect(busy.error == "A selection dialog is already open.")
        newPicker.finish(.success([.init(name: "current.txt", data: Data([2]))]))
        let result = try await newRequest.value
        #expect(result.error == nil)
        #expect(result.files.map(\.name) == ["current.txt"])
    }

    @Test func aTimedOutRequestReleasesItsPickerAndAllowsRetry() async throws {
        let window = PickerTestWindow(contentRect: .zero, styleMask: [], backing: .buffered, defer: true)
        let stalled = ControlledPicker()
        let retry = ControlledPicker(result: .success([.init(name: "retry.txt", data: Data())]))
        var pickers = [stalled, retry]
        let broker = RendererFileImport(timeout: 0.1, makePicker: { pickers.removeFirst() })
        let tabId = UUID()
        defer { stalled.finish(.success([])) }
        let pending = Task { try await request(broker, tabId: tabId, reloadKey: 1) { window } }
        try await stalled.waitUntilStarted()
        let result = try await pending.value
        #expect(result.error == "The file selection timed out.")
        #expect(result.files.isEmpty)
        #expect(stalled.cancelled)
        let selected = try await request(broker, tabId: tabId, reloadKey: 1) { window }
        #expect(selected.error == nil)
        #expect(selected.files.map(\.name) == ["retry.txt"])
    }

    @Test func closingTheTabReleasesItsRequestWithoutWaitingForTheProvider() async throws {
        let window = PickerTestWindow(contentRect: .zero, styleMask: [], backing: .buffered, defer: true)
        let picker = ControlledPicker()
        let broker = RendererFileImport(makePicker: { picker })
        let tabId = UUID()
        defer { picker.finish(.success([])) }
        let pending = Task { try await request(broker, tabId: tabId, reloadKey: 1) { window } }
        try await picker.waitUntilStarted()
        broker.update(tabId: tabId, reloadKey: 1)
        #expect(!picker.cancelled)
        broker.cancel(tabId: tabId)
        let result = try await pending.value
        #expect(picker.cancelled)
        #expect(result.error != nil)
        #expect(result.files.isEmpty)
    }

    @Test func revokingAuthorizationCancelsAnInFlightSelection() async throws {
        let window = PickerTestWindow(contentRect: .zero, styleMask: [], backing: .buffered, defer: true)
        let picker = ControlledPicker()
        let broker = RendererFileImport(makePicker: { picker })
        let tabId = UUID()
        var authorized = true
        defer { picker.finish(.success([])) }
        let pending = Task {
            try await request(broker, tabId: tabId, reloadKey: 1) { authorized ? window : nil }
        }
        try await picker.waitUntilStarted()
        authorized = false
        broker.update(tabId: tabId, reloadKey: 1)
        let result = try await pending.value
        #expect(picker.cancelled)
        #expect(result.error != nil)
        #expect(result.files.isEmpty)
    }

    @Test func lateRevocationPreventsDeliveryOfAlreadyReadFiles() async throws {
        let window = PickerTestWindow(contentRect: .zero, styleMask: [], backing: .buffered, defer: true)
        let picker = ControlledPicker()
        let broker = RendererFileImport(makePicker: { picker })
        let tabId = UUID()
        var authorized = true
        let pending = Task {
            try await request(broker, tabId: tabId, reloadKey: 1) { authorized ? window : nil }
        }
        try await picker.waitUntilStarted()
        authorized = false
        picker.finish(.success([.init(name: "private.txt", data: Data([1]))]))
        let result = try await pending.value
        #expect(result.error == "File import authorization changed.")
        #expect(result.files.isEmpty)
    }

    @Test func cancellationAndReadFailuresAllowTheNextSelection() async throws {
        let window = PickerTestWindow(contentRect: .zero, styleMask: [], backing: .buffered, defer: true)
        let cancelled = ControlledPicker(result: .success([]))
        let failed = ControlledPicker(result: .failure(CocoaError(.fileReadNoSuchFile)))
        let selected = ControlledPicker(result: .success([.init(name: "retry.txt", data: Data())]))
        var pickers = [cancelled, failed, selected]
        let broker = RendererFileImport(makePicker: { pickers.removeFirst() })
        let tabId = UUID()
        let first = try await request(broker, tabId: tabId, reloadKey: 1) { window }
        #expect(first.error == nil && first.files.isEmpty)
        let second = try await request(broker, tabId: tabId, reloadKey: 1) { window }
        #expect(second.error == "Could not read the selected items.")
        let third = try await request(broker, tabId: tabId, reloadKey: 1) { window }
        #expect(third.error == nil)
        #expect(third.files.map(\.name) == ["retry.txt"])
    }

    private func request(
        _ broker: RendererFileImport, tabId: UUID, reloadKey: Int,
        authorizedWindow: @escaping () -> NSWindow?
    ) async throws -> GuestFileImportReply {
        let call = GuestFileImportCall(reloadKey: reloadKey, source: .photos, accept: [], multiple: true)
        let json = String(decoding: try JSONEncoder().encode(call), as: UTF8.self)
        let response = try await broker.reply(to: json, tabId: tabId) { _ in authorizedWindow() }
        let encoded = try #require(response)
        return try JSONDecoder().decode(GuestFileImportReply.self, from: Data(encoded.utf8))
    }
}

@MainActor
private final class PickerTestWindow: NSWindow {
    override var isKeyWindow: Bool { true }
}

@MainActor
private final class ControlledPicker: FileImportPicking {
    private var continuation: CheckedContinuation<[GuestFileImportReply.File], Error>?
    private let result: Result<[GuestFileImportReply.File], Error>?
    private var started = false
    private(set) var cancelled = false

    init(result: Result<[GuestFileImportReply.File], Error>? = nil) {
        self.result = result
    }

    func pick(_ call: GuestFileImportCall, in window: NSWindow) async throws -> [GuestFileImportReply.File] {
        started = true
        if let result { return try result.get() }
        return try await withCheckedThrowingContinuation { continuation = $0 }
    }

    // Keep the continuation alive to simulate a provider that calls back after cancellation.
    func cancel() { cancelled = true }

    func finish(_ result: Result<[GuestFileImportReply.File], Error>) {
        let pending = continuation
        continuation = nil
        pending?.resume(with: result)
    }

    func waitUntilStarted() async throws {
        let deadline = ContinuousClock.now + .seconds(1)
        while !started && ContinuousClock.now < deadline {
            await Task.yield()
        }
        try #require(started)
    }
}
