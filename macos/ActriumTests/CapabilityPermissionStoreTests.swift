import XCTest
@testable import Actrium

@MainActor
final class CapabilityPermissionStoreTests: XCTestCase {
    private var appId: String!
    private var tabId: UUID!

    override func setUp() async throws {
        appId = "test/capability-\(UUID().uuidString)"
        tabId = UUID()
    }

    override func tearDown() async throws {
        _ = CapabilityPermissionStore.shared.revoke(
            appId: appId,
            capability: "host:clipboard.write"
        )
        CapabilityPermissionStore.shared.clearSessionGrants(tabId: tabId)
    }

    func testSessionGrantIsVisibleUntilSessionClears() throws {
        let store = CapabilityPermissionStore.shared
        XCTAssertTrue(
            store.decide(
                tabId: tabId,
                appId: appId,
                version: "1.0.0",
                capability: "host:clipboard.write",
                choice: .allowForSession
            )
        )
        XCTAssertEqual(
            grantStatus(in: snapshot(from: store)),
            "granted"
        )

        store.clearSessionGrants(tabId: tabId)
        XCTAssertNil(grantStatus(in: snapshot(from: store)))
    }

    func testAlwaysAllowAndDenyPersist() throws {
        let store = CapabilityPermissionStore.shared
        XCTAssertTrue(
            store.decide(
                tabId: tabId,
                appId: appId,
                version: "1.0.0",
                capability: "host:clipboard.write",
                choice: .alwaysAllow
            )
        )
        XCTAssertEqual(grantStatus(in: snapshot(from: store)), "granted")

        XCTAssertTrue(
            store.decide(
                tabId: tabId,
                appId: appId,
                version: "1.0.0",
                capability: "host:clipboard.write",
                choice: .deny
            )
        )
        XCTAssertEqual(grantStatus(in: snapshot(from: store)), "denied")
    }

    func testSessionAllowDoesNotOverridePersistedDeny() throws {
        let store = CapabilityPermissionStore.shared
        XCTAssertTrue(
            store.decide(
                tabId: tabId,
                appId: appId,
                version: "1.0.0",
                capability: "host:clipboard.write",
                choice: .deny
            )
        )
        XCTAssertTrue(
            store.decide(
                tabId: tabId,
                appId: appId,
                version: "1.0.0",
                capability: "host:clipboard.write",
                choice: .allowForSession
            )
        )
        XCTAssertEqual(grantStatus(in: snapshot(from: store)), "denied")
    }

    func testUndeclaredCapabilityIsExcludedFromSnapshot() throws {
        let store = CapabilityPermissionStore.shared
        XCTAssertTrue(
            store.decide(
                tabId: tabId,
                appId: appId,
                version: "1.0.0",
                capability: "host:clipboard.write",
                choice: .alwaysAllow
            )
        )
        let json = try XCTUnwrap(
            store.snapshotJSON(
                tabId: tabId,
                appId: appId,
                version: "1.0.0",
                declaredCapabilities: ["service"]
            )
        )
        let data = try XCTUnwrap(json.data(using: .utf8))
        let snapshot = try XCTUnwrap(
            JSONSerialization.jsonObject(with: data) as? [String: Any]
        )
        XCTAssertNil(grantStatus(in: snapshot))
    }

    func testUnknownEffectIsNotTreatedAsGranted() throws {
        DatabaseManager.shared.execute(
            """
            INSERT INTO capability_grants
                (app_id, capability, effect, lifetime, source, version, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(app_id, capability) DO UPDATE SET
                effect = excluded.effect,
                updated_at = excluded.updated_at
            """,
            params: [
                appId,
                "host:clipboard.write",
                "unexpected-effect",
                "service",
                "user",
                nil,
                Int(Date().timeIntervalSince1970)
            ]
        )
        XCTAssertNil(grantStatus(in: snapshot(from: CapabilityPermissionStore.shared)))
    }

    func testModuleDeclarationCoversMethodGrant() throws {
        let store = CapabilityPermissionStore.shared
        XCTAssertTrue(
            store.decide(
                tabId: tabId,
                appId: appId,
                version: "1.0.0",
                capability: "host:clipboard.write",
                choice: .alwaysAllow
            )
        )
        XCTAssertEqual(
            grantStatus(
                in: snapshot(
                    from: store,
                    declaredCapabilities: ["host:clipboard"]
                )
            ),
            "granted"
        )
    }

    func testModuleDeclarationCoversMethodDeny() throws {
        let store = CapabilityPermissionStore.shared
        XCTAssertTrue(
            store.decide(
                tabId: tabId,
                appId: appId,
                version: "1.0.0",
                capability: "host:clipboard.write",
                choice: .deny
            )
        )
        XCTAssertEqual(
            grantStatus(
                in: snapshot(
                    from: store,
                    declaredCapabilities: ["host:clipboard"]
                )
            ),
            "denied"
        )
    }

    func testSessionGrantIsScopedToTheSubject() throws {
        let store = CapabilityPermissionStore.shared
        let otherAppId = "test/other-\(UUID().uuidString)"
        XCTAssertTrue(
            store.decide(
                tabId: tabId,
                appId: appId,
                version: "1.0.0",
                capability: "host:clipboard.write",
                choice: .allowForSession
            )
        )
        let otherSnapshot = try XCTUnwrap(
            snapshot(
                from: store,
                subjectAppId: otherAppId,
                declaredCapabilities: ["host:clipboard.write"]
            )
        )
        XCTAssertNil(grantStatus(in: otherSnapshot))
    }

    func testSnapshotHasContractShape() throws {
        let snapshot = try XCTUnwrap(snapshot(from: CapabilityPermissionStore.shared))
        XCTAssertEqual(snapshot["schemaVersion"] as? Int, 1)
        XCTAssertNotNil(snapshot["revision"] as? Int)
        XCTAssertEqual(
            snapshot["declaredCapabilities"] as? [String],
            ["service", "host:clipboard.write"]
        )
        let session = try XCTUnwrap(snapshot["session"] as? [String: Any])
        XCTAssertEqual(session["state"] as? String, "active")
        XCTAssertEqual(
            session["allowedCapabilities"] as? [String],
            ["service", "host:clipboard.write"]
        )
    }

    private func snapshot(
        from store: CapabilityPermissionStore,
        subjectAppId: String? = nil,
        declaredCapabilities: [String] = ["service", "host:clipboard.write"]
    ) -> [String: Any]? {
        guard let json = store.snapshotJSON(
            tabId: tabId,
            appId: subjectAppId ?? appId,
            version: "1.0.0",
            declaredCapabilities: declaredCapabilities
        ), let data = json.data(using: .utf8) else {
            return nil
        }
        return try? JSONSerialization.jsonObject(with: data) as? [String: Any]
    }

    private func grantStatus(in snapshot: [String: Any]?) -> String? {
        (snapshot?["grants"] as? [String: String])?["host:clipboard.write"]
    }
}
