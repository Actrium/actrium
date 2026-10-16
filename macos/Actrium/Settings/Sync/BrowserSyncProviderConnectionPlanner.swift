import Foundation
import ActriumCore

enum BrowserSyncProviderRouting {
    // No provider is configured: browser sync falls through to the
    // endpoint-token reference-server branch (development path).
    case notConfigured

    // The device has not joined the cloud store yet (no persisted membership
    // marker): run the one-time membership join round first.
    case runLocalFolderMembership

    // Connection is ready: run the local-folder sync cycle round.
    case runLocalFolderCycle

    // The connection status policy demands setup, user action, or a retry
    // before any round may run.
    case blocked
}

struct BrowserSyncProviderConnectionPlan {
    let routing: BrowserSyncProviderRouting
    let providerSnapshot: [String: Any]?
    let connectionPolicy: [String: Any]?
}

// Assembles the provider snapshot for the Rust cloud provider connection
// contract and derives the coordinator routing from the resulting policy.
// Pure by design: no IO — the accept-time folder preflight and each round's
// own capability/compatibility preflights are the authoritative folder
// checks, so the snapshot is fully determined by persisted settings plus
// in-memory session state. This mirrors the Windows
// BrowserSyncProviderConnectionPlanner so both platforms route over the exact
// same Rust connection-status-policy contract.
enum BrowserSyncProviderConnectionPlanner {
    static func isConfigured(_ settings: BrowserSyncProviderSettingsSnapshot?) -> Bool {
        settings?.isConfigured ?? false
    }

    static func buildProviderSnapshot(
        _ settings: BrowserSyncProviderSettingsSnapshot,
        lastProviderErrorKind: String?
    ) -> [String: Any] {
        var snapshot: [String: Any] = [
            "providerKind": settings.providerKind ?? "",
            "scope": settings.scope,
            "deviceId": settings.deviceId,
        ]

        if settings.providerKind == BrowserSyncProviderSettingsStore.localFolderProviderKind {
            // Local folder is connection-free: there is no session to open,
            // so a configured folder is by definition "connected", and the
            // credential state is constantly "available" because there are
            // no secrets. (The connection contract normalizes unknown
            // connection tokens to "disconnected", which would misclassify a
            // configured local folder as signInRequired.)
            snapshot["connectionState"] = "connected"
            snapshot["credentialState"] = "available"
            // The persisted membership marker is the only cold-start
            // membership signal: a membership round is a write to the shared
            // cloud folder, and repeating it on every launch is wrong even
            // though the device-document upsert is idempotent. "missing"
            // routes this run to a one-time join; after a successful join the
            // marker persists and later launches go straight to the cycle.
            snapshot["membershipStatus"] =
                settings.membershipJoinedAtUnixSeconds != nil ? "active" : "missing"
            // No IO here: the folder was validated when it was accepted and
            // the cycle round re-validates it authoritatively, so the store
            // is reported ready and a real storage failure comes back as a
            // storage-scoped provider error on the next round.
            snapshot["cloudStoreReady"] = true
        }

        if let lastProviderErrorKind, !lastProviderErrorKind.isEmpty {
            snapshot["lastProviderErrorKind"] = lastProviderErrorKind
        }

        return snapshot
    }

    static func plan(
        settings: BrowserSyncProviderSettingsSnapshot?,
        lastProviderErrorKind: String?,
        retryAttempt: UInt32,
        observedAtUnixSeconds: Int64
    ) -> BrowserSyncProviderConnectionPlan {
        guard let settings, isConfigured(settings) else {
            // Short-circuit without calling the policy: an unconfigured
            // provider is the endpoint branch's territory, not a blocked
            // provider connection.
            return BrowserSyncProviderConnectionPlan(
                routing: .notConfigured,
                providerSnapshot: nil,
                connectionPolicy: nil
            )
        }

        let providerSnapshot = buildProviderSnapshot(settings, lastProviderErrorKind: lastProviderErrorKind)
        let policyJson = browserStorageSyncCloudProviderConnectionStatusPolicyJson(
            providerSnapshotJson: jsonString(providerSnapshot) ?? "{}",
            retryAttempt: retryAttempt,
            observedAtUnixSeconds: observedAtUnixSeconds
        )
        // The Rust core owns the connection classification and action rules.
        // A result that is not a JSON object is contract drift and falls back
        // to Blocked rather than a hand-copied policy.
        let connectionPolicy = jsonObject(policyJson)

        let isLocalFolder = settings.providerKind == BrowserSyncProviderSettingsStore.localFolderProviderKind
        let routing: BrowserSyncProviderRouting
        if isLocalFolder, jsonBool(connectionPolicy?["shouldAttemptMembership"]) {
            routing = .runLocalFolderMembership
        } else if isLocalFolder, jsonBool(connectionPolicy?["shouldAttemptSync"]) {
            routing = .runLocalFolderCycle
        } else {
            routing = .blocked
        }

        return BrowserSyncProviderConnectionPlan(
            routing: routing,
            providerSnapshot: providerSnapshot,
            connectionPolicy: connectionPolicy
        )
    }

    // Translates the connection status policy output (schedulerAction /
    // retryDelaySeconds / userAction / resetRetryAttempt) into the field
    // shape the sync scheduler consumes. No round envelope exists on the
    // Blocked routing, so this pure translation is the only scheduleState
    // source for blocked provider connections.
    static func buildBlockedScheduleState(
        connectionPolicy: [String: Any],
        retryAttempt: UInt32,
        observedAtUnixSeconds: Int64
    ) -> [String: Any] {
        let schedulerAction = jsonStringValue(connectionPolicy["schedulerAction"])
        let userAction = jsonStringValue(connectionPolicy["userAction"])
        let requiresUserAction = !userAction.isEmpty && userAction != "none"

        var scheduleState: [String: Any] = [
            "planType": "cloudProviderConnectionScheduleState",
            "classification": jsonStringValue(connectionPolicy["classification"]),
            "schedulerAction": schedulerAction,
            // Local folder never waits for credentials; cloud providers that
            // do surface it through their own classifications.
            "waitingForCredentials": false,
            "requiresUserAction": requiresUserAction,
            "userAction": userAction,
            "resetRetryAttempt": jsonBool(connectionPolicy["resetRetryAttempt"]),
        ]

        if let retryDelaySeconds = jsonUInt(connectionPolicy["retryDelaySeconds"]) {
            scheduleState["nextRunDelaySeconds"] = retryDelaySeconds
            scheduleState["nextRunAtUnixSeconds"] = observedAtUnixSeconds + Int64(retryDelaySeconds)
        }

        if schedulerAction == "scheduleRetry" {
            scheduleState["nextRetryAttempt"] = retryAttempt + 1
        }

        return scheduleState
    }

    // Maps the provider error classification a round's diagnostics reports
    // into the error-kind vocabulary the connection contract matches on so
    // the next planner run classifies the connection truthfully within this
    // session.
    static func mapProviderErrorClassification(_ providerErrorClassification: String?) -> String? {
        switch providerErrorClassification {
        case nil, "", "ready":
            return nil
        case "authExpired":
            return "auth"
        case "quotaExceeded":
            return "quota"
        case "storageSetupRequired":
            return "storage"
        // Rereads are handled inside the rounds themselves; not a
        // connection-level condition.
        case "cloudStateRereadRequired":
            return nil
        // Unknown tokens pass through; the connection contract ignores kinds
        // it does not match.
        default:
            return providerErrorClassification
        }
    }

    // Membership- or storage-scoped provider errors mean the shared cloud
    // store no longer knows this device (folder wiped, store reset by another
    // device): clearing the persisted membership marker makes the next run
    // re-join. A wrongly cleared marker costs exactly one corrective
    // membership round, so this stays deliberately narrow.
    static func shouldClearMembershipMarker(_ providerErrorClassification: String?) -> Bool {
        switch providerErrorClassification {
        case "storageSetupRequired", "recoveryRequired":
            return true
        case "membershipRequired", "deviceMembershipMissing", "deviceMembershipRevoked":
            return true
        default:
            return false
        }
    }

    // MARK: - JSON helpers

    static func jsonObject(_ json: String) -> [String: Any]? {
        guard json != "null", let data = json.data(using: .utf8) else { return nil }
        return try? JSONSerialization.jsonObject(with: data) as? [String: Any]
    }

    static func jsonString(_ value: [String: Any]) -> String? {
        guard JSONSerialization.isValidJSONObject(value),
              let data = try? JSONSerialization.data(withJSONObject: value, options: [.sortedKeys])
        else {
            return nil
        }
        return String(data: data, encoding: .utf8)
    }

    static func jsonStringValue(_ value: Any?) -> String {
        value as? String ?? ""
    }

    static func jsonBool(_ value: Any?) -> Bool {
        if let value = value as? Bool { return value }
        if let value = value as? NSNumber { return value.boolValue }
        return false
    }

    static func jsonUInt(_ value: Any?) -> UInt32? {
        if let value = value as? NSNumber {
            let intValue = value.intValue
            return intValue >= 0 ? UInt32(intValue) : nil
        }
        return nil
    }
}
