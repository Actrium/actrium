import Foundation

enum CapabilityPermissionChoice {
    case allowForSession
    case alwaysAllow
    case deny
}

/// Native-owned capability grant store. Keel only receives immutable snapshots.
@MainActor
final class CapabilityPermissionStore {
    static let shared = CapabilityPermissionStore()

    private let database = DatabaseManager.shared
    private var sessionAllows: [UUID: [String: Set<String>]] = [:]
    private var revision: Int64 = Int64(Date().timeIntervalSince1970 * 1000)

    private init() {
        createTable()
    }

    private func createTable() {
        let created = database.execute(
            """
            CREATE TABLE IF NOT EXISTS capability_grants (
                app_id TEXT NOT NULL,
                capability TEXT NOT NULL,
                effect TEXT NOT NULL,
                lifetime TEXT NOT NULL,
                source TEXT NOT NULL,
                version TEXT,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (app_id, capability)
            )
            """
        )
        if !created {
            AppLogger.database.fault("Failed to create capability_grants table")
        }
    }

    func snapshotJSON(
        tabId: UUID,
        appId: String?,
        version: String?,
        declaredCapabilities: [String]
    ) -> String? {
        let appId = appId?.trimmingCharacters(in: .whitespacesAndNewlines)
        let subjectAppId = (appId?.isEmpty == false) ? appId! : tabId.uuidString
        let grants = grantStatuses(
            tabId: tabId,
            appId: subjectAppId,
            declaredCapabilities: declaredCapabilities
        )
        let subjectParts = subjectAppId.split(separator: "/", maxSplits: 1).map(String.init)
        let snapshot: [String: Any] = [
            "schemaVersion": 1,
            "revision": Int(revision),
            "subject": [
                "publisher": subjectParts.count > 1 ? subjectParts[0] : "local",
                "program": subjectParts.count > 1 ? subjectParts[1] : subjectAppId,
                "majorVersion": majorVersion(version),
                "packageDigest": ""
            ],
            "audience": "third_party",
            "declaredCapabilities": declaredCapabilities,
            "grants": grants,
            "platform": [:],
            "session": [
                "generation": 1,
                "state": "active",
                "allowedCapabilities": declaredCapabilities
            ]
        ]

        guard JSONSerialization.isValidJSONObject(snapshot),
              let data = try? JSONSerialization.data(
                withJSONObject: snapshot,
                options: [.sortedKeys]
              ) else {
            return nil
        }
        return String(data: data, encoding: .utf8)
    }

    @discardableResult
    func decide(
        tabId: UUID,
        appId: String?,
        version: String?,
        capability: String,
        choice: CapabilityPermissionChoice
    ) -> Bool {
        let subjectAppId = normalizedAppId(appId, fallback: tabId)
        switch choice {
        case .allowForSession:
            sessionAllows[tabId, default: [:]][subjectAppId, default: []].insert(capability)
            touchRevision()
            return true
        case .alwaysAllow:
            let stored = upsertGrant(
                appId: subjectAppId,
                capability: capability,
                effect: "allow",
                lifetime: "service",
                source: "user",
                version: version
            )
            if stored {
                sessionAllows[tabId, default: [:]][subjectAppId, default: []].insert(capability)
                touchRevision()
            }
            return stored
        case .deny:
            let stored = upsertGrant(
                appId: subjectAppId,
                capability: capability,
                effect: "deny",
                lifetime: "service",
                source: "user",
                version: version
            )
            if stored {
                sessionAllows[tabId]?[subjectAppId]?.remove(capability)
                touchRevision()
            }
            return stored
        }
    }

    @discardableResult
    func revoke(appId: String, capability: String) -> Bool {
        let deleted = database.execute(
            "DELETE FROM capability_grants WHERE app_id = ? AND capability = ?",
            params: [appId, capability]
        )
        if deleted {
            for tabId in Array(sessionAllows.keys) {
                guard var perSubject = sessionAllows[tabId] else { continue }
                for subject in Array(perSubject.keys) {
                    perSubject[subject]?.remove(capability)
                }
                sessionAllows[tabId] = perSubject
            }
            touchRevision()
        }
        return deleted
    }

    func clearSessionGrants(tabId: UUID) {
        guard sessionAllows.removeValue(forKey: tabId) != nil else { return }
        touchRevision()
    }

    /// A declaration covers a method when it is the exact method id or the
    /// module alias (`host:clipboard` covers `host:clipboard.write`).
    private func declarationCovers(
        _ capability: String,
        declarations: [String]
    ) -> Bool {
        declarations.contains { declaration in
            capability == declaration || capability.hasPrefix(declaration + ".")
        }
    }

    private func grantStatuses(
        tabId: UUID,
        appId: String,
        declaredCapabilities: [String]
    ) -> [String: String] {
        var result: [String: String] = [:]
        let rows = database.query(
            "SELECT capability, effect FROM capability_grants WHERE app_id = ?",
            params: [appId]
        )
        for row in rows {
            guard let capability = row["capability"] as? String,
                  declarationCovers(capability, declarations: declaredCapabilities) else {
                continue
            }
            let effect = row["effect"] as? String
            if effect == "deny" {
                result[capability] = "denied"
            } else if effect == "allow" {
                result[capability] = "granted"
            }
        }
        for capability in sessionAllows[tabId]?[appId] ?? []
        where declarationCovers(capability, declarations: declaredCapabilities) {
            if result[capability] != "denied" {
                result[capability] = "granted"
            }
        }
        return result
    }

    private func upsertGrant(
        appId: String,
        capability: String,
        effect: String,
        lifetime: String,
        source: String,
        version: String?
    ) -> Bool {
        database.execute(
            """
            INSERT INTO capability_grants
                (app_id, capability, effect, lifetime, source, version, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(app_id, capability) DO UPDATE SET
                effect = excluded.effect,
                lifetime = excluded.lifetime,
                source = excluded.source,
                version = excluded.version,
                updated_at = excluded.updated_at
            """,
            params: [
                appId,
                capability,
                effect,
                lifetime,
                source,
                version,
                Int(Date().timeIntervalSince1970)
            ]
        )
    }

    private func normalizedAppId(_ appId: String?, fallback tabId: UUID) -> String {
        let value = appId?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return value.isEmpty ? tabId.uuidString : value
    }

    private func majorVersion(_ version: String?) -> Int {
        guard let value = version?.split(separator: ".").first,
              let major = Int(value) else {
            return 1
        }
        return major
    }

    private func touchRevision() {
        revision = max(
            Int64(Date().timeIntervalSince1970 * 1000),
            revision + 1
        )
    }
}
