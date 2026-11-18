import ActriumIPC
import AppKit
import Combine
import Foundation
import Testing
@testable import Actrium

@MainActor
struct RendererFileImportTests {
    @Test func requiresBothTheFrontDeclarationAndPickerUsage() {
        let manifest = #"{"hostModules":{"file-import":{"rpc":["pick"]}}}"#
        let capability = GuestFileImportCall.capability
        #expect(RendererFileImport.isDeclared(manifest: manifest, permissions: [capability]))
        #expect(RendererFileImport.isDeclared(manifest: manifest, permissions: ["host:file-import"]))
        #expect(!RendererFileImport.isDeclared(manifest: manifest, permissions: ["host:file-export.save"]))
        #expect(!RendererFileImport.isDeclared(manifest: #"{"hostModules":{}}"#, permissions: [capability]))
    }

    @Test func rejectsTheRequestForAnUnavailableDocument() async throws {
        let call = GuestFileImportCall(reloadKey: 4, source: .photos, accept: [], multiple: true)
        let json = String(decoding: try JSONEncoder().encode(call), as: UTF8.self)
        let reply = try await RendererFileImport().reply(to: json, tabId: UUID()) { reloadKey in
            #expect(reloadKey == call.reloadKey)
            return nil
        }
        let encoded = try #require(reply)
        let decoded = try JSONDecoder().decode(GuestFileImportReply.self, from: Data(encoded.utf8))
        #expect(decoded.id == call.id)
        #expect(decoded.error != nil)
        #expect(decoded.files.isEmpty)
    }

    @Test func oversizedRequestsReturnACorrelatedErrorBeforeAuthorization() async throws {
        let call = GuestFileImportCall(reloadKey: 1, source: .photos, accept: [], multiple: true)
        let data = try JSONEncoder().encode(call)
        var object = try #require(JSONSerialization.jsonObject(with: data) as? [String: Any])
        object["padding"] = String(repeating: "x", count: GuestFileImportCall.maxRequestBytes)
        let oversized = String(decoding: try JSONSerialization.data(withJSONObject: object), as: UTF8.self)
        var authorizationRequested = false
        let reply = try await RendererFileImport().reply(to: oversized, tabId: UUID()) { _ in
            authorizationRequested = true
            return nil
        }
        let encoded = try #require(reply)
        let decoded = try JSONDecoder().decode(GuestFileImportReply.self, from: Data(encoded.utf8))
        #expect(decoded.id == call.id)
        #expect(decoded.error == "The file import request exceeds 32 KiB.")
        #expect(decoded.files.isEmpty)
        #expect(!authorizationRequested)
    }

    @Test func aSessionGrantIsScopedToItsTabAndCanBeCleared() {
        let tabId = UUID()
        let appId = "test-file-import/\(UUID().uuidString)"
        let capability = GuestFileImportCall.capability
        let store = CapabilityPermissionStore.shared
        defer { store.clearSessionGrants(tabId: tabId) }
        #expect(!store.hasGrant(tabId: tabId, appId: appId, capability: capability))
        #expect(store.decide(tabId: tabId, appId: appId, version: nil, capability: capability, choice: .allowForSession))
        #expect(store.hasGrant(tabId: tabId, appId: appId, capability: capability))
        #expect(!store.hasGrant(tabId: UUID(), appId: appId, capability: capability))
        store.clearSessionGrants(tabId: tabId)
        #expect(!store.hasGrant(tabId: tabId, appId: appId, capability: capability))
    }

    @Test func rendererReceivesNativeRejectionWhenThereIsNoAuthorizedWindow() async throws {
        let tabId = UUID()
        let origin = "test:file-import:\(tabId.uuidString)"
        let pool = RendererProcessPool.shared
        let store = CapabilityPermissionStore.shared
        let capability = GuestFileImportCall.capability
        let permissions = [capability]
        let expected = "File import is unavailable for this tab."
        var received = false
        let updates = NotificationCenter.default.publisher(for: .rendererAccessibilityUpdated).sink { notification in
            guard let tree = notification.userInfo?["tree"] as? Actrium_Ipc_AccessibilityTree,
                  tree.tabID == tabId.uuidString else { return }
            received = tree.nodes.contains { $0.label.contains(expected) }
        }
        defer {
            updates.cancel()
            pool.releaseRenderer(for: tabId)
            store.clearSessionGrants(tabId: tabId)
        }
        #expect(store.decide(tabId: tabId, appId: nil, version: nil, capability: capability, choice: .allowForSession))
        let snapshot = try #require(store.snapshotJSON(tabId: tabId, appId: nil, version: nil, declaredCapabilities: permissions))
        _ = try pool.allocateRenderer(for: tabId, fingerprint: origin)
        try await pool.prepareTab(tabId: tabId, config: [
            "title": "File Import Probe", "unifiedScript": Self.probeBundle,
            "containerWidth": 800, "containerHeight": 500, "reloadKey": 3,
            "permissions": permissions,
            "capabilitySnapshotJSON": snapshot, "capabilityManifestJSON": Self.probeManifest,
        ])

        let deadline = ContinuousClock.now + .seconds(8)
        while !received && ContinuousClock.now < deadline {
            pool.requestAccessibilityTree(for: tabId)
            try await Task.sleep(for: .milliseconds(100))
        }
        #expect(received)
    }

    private static let probeManifest = """
    {"schemaVersion":1,"keelApi":"1.0","guestEntry":"workload/guest.js",
     "imports":["host:file-import","keel/guest","react"],"components":["Text"],
     "hostModules":{"file-import":{"rpc":["pick"]}}}
    """

    private static let probeBundle = """
    // keel:v1-linkage {"guestEntry":"workload/guest.js","links":[["host:file-import","pick"],["keel/guest","Text"],["react"]]}
    var React = require('react');
    var Text = require('keel/guest').Text;
    var picker = globalThis.__keel.hostModules['host:file-import'];
    function FileImportProbe() {
      var report = React.useState('Waiting for the picker.');
      React.useEffect(function () {
        picker.pick({ source: 'photos', multiple: true }).then(function () {
          report[1]('Unexpected file selection.');
        }, function (error) {
          report[1](String(error.message || error));
        });
      }, []);
      return React.createElement(Text, { accessible: true, accessibilityLabel: report[0] }, report[0]);
    }
    module.exports = { default: FileImportProbe };
    """
}
