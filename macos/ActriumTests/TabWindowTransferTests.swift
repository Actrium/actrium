import ActriumCore
import ActriumIPC
import AppKit
import Foundation
import Testing
@testable import Actrium

@MainActor
struct TabWindowTransferTests {
    @Test func crossWindowDropPreservesStorageAndUsesANewDocument() async throws {
        let original = storageTab(origin: "signed:publisher:counter")
        let source = TransferWindow(tabs: [original])
        let destination = TransferWindow()
        let outline = TabTransferOutlineView()
        outline.transferOwner = source
        let nativeSource = try #require(TabWindowTransfer.source(for: outline))
        source.beforeRemoval = { #expect(destination.tabs.count == 1) }
        #expect(TabWindowTransfer.moveTab(
            original.id, from: nativeSource, to: destination, at: 0, groupId: nil
        ))
        let moved = try #require(destination.tabs.first)
        #expect(source.tabs.isEmpty)
        try await verifyStorage(original: original, moved: moved)
    }

    @Test func groupToNewWindowPreservesEachOriginAndGroup() async throws {
        let group = TabGroup(name: "Apps", color: .blue)
        let first = storageTab(origin: "signed:publisher:counter", groupId: group.id)
        let second = storageTab(origin: "dev:/another-app", groupId: group.id)
        let unrelated = storageTab(origin: "signed:publisher:unrelated")
        let source = TransferWindow(tabs: [first, second, unrelated])
        let destination = TransferWindow()
        source.beforeRemoval = { #expect(destination.tabs.count == 2) }
        let transfer = TabGroupWindowTransfer(source: source, group: group)
        #expect(transfer.move(to: destination))
        #expect(source.tabs.map(\.id) == [unrelated.id])
        #expect(destination.group?.name == "Apps")
        #expect(destination.group?.color == .blue)
        #expect(destination.tabs.allSatisfy { $0.groupId == destination.group?.id })
        try await verifyStorage(original: first, moved: destination.tabs[0])
        try await verifyStorage(original: second, moved: destination.tabs[1])
        #expect(!transfer.move(to: TransferWindow()))
    }

    @Test func missingSourcesAndInvalidRequestsLeaveTabsIntact() {
        let tab = storageTab(origin: "signed:publisher:counter")
        let source = TransferWindow(tabs: [tab])
        #expect(!TabWindowTransfer.moveTab(UUID(), from: source, to: TransferWindow(), at: 0, groupId: nil))
        #expect(!TabWindowTransfer.moveTab(tab.id, from: source, to: source, at: 0, groupId: nil))
        let request = TabGroupWindowTransfer(source: source, group: TabGroup())
        #expect(source.tabs.map(\.id) == [tab.id])
        #expect(!request.move(to: TransferWindow()))
        let vanished: TabGroupWindowTransfer
        do {
            let temporary = TransferWindow(tabs: [tab])
            vanished = TabGroupWindowTransfer(source: temporary, group: TabGroup())
        }
        #expect(!vanished.move(to: TransferWindow()))
        #expect(source.tabs.map(\.id) == [tab.id])
    }

    @Test func ordinaryTabsRemainWithoutStorageIdentity() {
        let tab = Tab(title: "Page", url: URL(string: "https://example.com"))
        let source = TransferWindow(tabs: [tab])
        let destination = TransferWindow()
        #expect(TabWindowTransfer.moveTab(tab.id, from: source, to: destination, at: 0, groupId: nil))
        #expect(destination.tabs.first?.localStorageOrigin == nil)
        #expect(destination.tabs.first?.url == tab.url)
    }

    @Test func serializedOrForeignDragSourcesCannotProvideIdentity() {
        let forged = ["localStorageOrigin": "signed:another:app", "sourceTabId": UUID().uuidString]
        #expect(TabWindowTransfer.source(for: forged) == nil)
        #expect(TabWindowTransfer.source(for: NSObject()) == nil)
        #expect(TabWindowTransfer.source(for: nil) == nil)
        let unowned = TabTransferOutlineView()
        #expect(TabWindowTransfer.source(for: unowned) == nil)
    }

    private func storageTab(origin: String, groupId: UUID? = nil) -> Tab {
        var tab = Tab(
            title: "Counter", appId: "example/counter@1.0.0", groupId: groupId,
            unifiedScript: "verified script", guestExtractedPath: "/verified/package",
            guestEntry: "workload/guest.js", capabilityManifestJSON: Self.manifest,
            localStorageOrigin: origin, localStoragePublisherIdentity: "host-publisher-context", capabilityRevision: 3,
            contractName: "counter", contractVersion: 1,
            permissions: ["host:local-storage.getItem", "host:local-storage.setItem"], reloadNonce: 2
        )
        tab.realmProfileSigner = ("realm@example.com", "profile-key")
        return tab
    }

    private func verifyStorage(original: Tab, moved: Tab) async throws {
        #expect(moved.id != original.id)
        #expect(moved.localStorageOrigin == original.localStorageOrigin)
        #expect(moved.localStoragePublisherIdentity == original.localStoragePublisherIdentity)
        #expect(moved.appId == original.appId)
        #expect(moved.realmProfileSigner?.realmName == original.realmProfileSigner?.realmName)
        #expect(moved.realmProfileSigner?.keyId == original.realmProfileSigner?.keyId)
        #expect(moved.unifiedScript == original.unifiedScript)
        #expect(moved.guestExtractedPath == original.guestExtractedPath)
        #expect(moved.guestEntry == original.guestEntry)
        #expect(moved.capabilityManifestJSON == original.capabilityManifestJSON)
        #expect(moved.permissions == original.permissions)
        #expect(moved.capabilityRevision == original.capabilityRevision)
        #expect(moved.reloadNonce == original.reloadNonce)
        let store = GuestLocalStorage(path: ":memory:")
        let broker = RendererLocalStorage(database: store)
        let oldDocument = try prepare(broker, tab: original)
        let write = try await call(broker, original.id, oldDocument, .setItem, value: "saved")
        #expect(write.error == nil)
        broker.remove(original.id)
        let newDocument = try prepare(broker, tab: moved)
        #expect(newDocument != oldDocument)
        let read = try await call(broker, moved.id, newDocument, .getItem)
        #expect(read.value == "saved" && read.error == nil)
        let stale = try await call(broker, original.id, oldDocument, .getItem)
        #expect(stale.error != nil)
        var other = moved
        other.localStorageOrigin = "signed:other:program"
        let otherDocument = try prepare(broker, tab: other)
        let isolated = try await call(broker, other.id, otherDocument, .getItem)
        #expect(isolated.value == nil && isolated.error == nil)
    }

    private func prepare(_ broker: RendererLocalStorage, tab: Tab) throws -> UUID {
        var config: [String: Any] = [
            "permissions": tab.permissions, "capabilityManifestJSON": tab.capabilityManifestJSON ?? "",
            "reloadKey": tab.reloadNonce,
        ]
        config["localStorageOrigin"] = tab.localStorageOrigin
        let prepared = broker.prepare(tabId: tab.id, config: config)
        return try #require((prepared["localStorageDocumentId"] as? String).flatMap(UUID.init(uuidString:)))
    }

    private func call(
        _ broker: RendererLocalStorage, _ tab: UUID, _ document: UUID,
        _ operation: GuestLocalStorageCall.Operation, value: String? = nil
    ) async throws -> GuestLocalStorageReply {
        let request = GuestLocalStorageCall(id: UUID(), documentId: document, operation: operation, key: "key", value: value)
        let json = String(decoding: try JSONEncoder().encode(request), as: UTF8.self)
        let reply = try #require(try await broker.reply(to: json, tabId: tab))
        return try JSONDecoder().decode(GuestLocalStorageReply.self, from: Data(reply.utf8))
    }

    private static let manifest = #"{"hostModules":{"local-storage":{"rpc":["getItem","setItem"]}}}"#
}

@MainActor
private final class TransferWindow: TabWindowTransferOwner {
    var tabs: [Tab]
    var group: TabGroup?
    var beforeRemoval: (() -> Void)?

    init(tabs: [Tab] = []) { self.tabs = tabs }

    func insertTabFromCrossWindow(_ tab: Tab, at index: Int, groupId: UUID?) {
        var tab = tab
        tab.groupId = groupId
        tabs.insert(tab, at: min(index, tabs.count))
    }

    func removeTabById(_ id: UUID) {
        beforeRemoval?()
        tabs.removeAll { $0.id == id }
    }

    func createGroupFromTabs(_ ids: [UUID], name: String, color: TabGroupColor) {
        let group = TabGroup(name: name, color: color)
        self.group = group
        for index in tabs.indices where ids.contains(tabs[index].id) { tabs[index].groupId = group.id }
    }
}
