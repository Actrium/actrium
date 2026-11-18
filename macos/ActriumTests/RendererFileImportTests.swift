import ActriumIPC
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
        defer {
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
        var result: String?
        while result == nil && ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(100))
            result = DevToolsManager.shared.consoleLogs.first {
                $0.source.contains(String(tabId.uuidString.prefix(8))) && $0.message.contains("File import is unavailable")
            }?.message
        }
        #expect(result?.contains("File import is unavailable for this tab.") == true)
    }

    private static let probeManifest = """
    {"schemaVersion":1,"keelApi":"1.0","guestEntry":"workload/guest.js",
     "imports":["host:file-import"],"components":[],
     "hostModules":{"file-import":{"rpc":["pick"]}}}
    """

    private static let probeBundle = """
    // keel:v1-linkage {"guestEntry":"workload/guest.js","links":[["host:file-import","pick"]]}
    var picker = globalThis.__keel.hostModules['host:file-import'];
    picker.pick({ source: 'photos', multiple: true }).then(function () {
      console.error('Unexpected file selection.');
    }, function (error) {
      console.error(String(error.message || error));
    });
    module.exports = { default: function FileImportProbe() { return null; } };
    """
}
