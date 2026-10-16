import Foundation

public struct AppleBrowserStorageSyncSchedulerSnapshot: Equatable, Sendable {
    public var retryAttempt: UInt32
    public var schedulerAction: String
    public var nextRunDelaySeconds: UInt32?
    public var nextRunAtUnixSeconds: Int64?
    public var waitingForCredentials: Bool
    public var requiresUserAction: Bool
    public var pendingRunScheduled: Bool
    public var lastResultJson: String?

    public init(
        retryAttempt: UInt32 = 0,
        schedulerAction: String = "idle",
        nextRunDelaySeconds: UInt32? = nil,
        nextRunAtUnixSeconds: Int64? = nil,
        waitingForCredentials: Bool = false,
        requiresUserAction: Bool = false,
        pendingRunScheduled: Bool = false,
        lastResultJson: String? = nil
    ) {
        self.retryAttempt = retryAttempt
        self.schedulerAction = schedulerAction
        self.nextRunDelaySeconds = nextRunDelaySeconds
        self.nextRunAtUnixSeconds = nextRunAtUnixSeconds
        self.waitingForCredentials = waitingForCredentials
        self.requiresUserAction = requiresUserAction
        self.pendingRunScheduled = pendingRunScheduled
        self.lastResultJson = lastResultJson
    }
}

public actor AppleBrowserStorageSyncScheduler {
    private let coordinator: AppleBrowserStorageSyncCoordinator
    private let schedulesFollowUp: Bool
    private var retryAttempt: UInt32 = 0
    private var pendingTask: Task<Void, Never>?
    private var snapshotValue = AppleBrowserStorageSyncSchedulerSnapshot()

    public init(
        coordinator: AppleBrowserStorageSyncCoordinator,
        schedulesFollowUp: Bool = true
    ) {
        self.coordinator = coordinator
        self.schedulesFollowUp = schedulesFollowUp
    }

    public func runNow(
        generatedAtUnixSeconds: Int64? = nil,
        limitPerCollection: Int = 200,
        observedAtUnixSeconds: Int64? = nil
    ) async -> String {
        cancelPendingSync()
        return await runCycle(
            generatedAtUnixSeconds: generatedAtUnixSeconds,
            limitPerCollection: limitPerCollection,
            observedAtUnixSeconds: observedAtUnixSeconds
        )
    }

    public func scheduleAfterLocalChange(delaySeconds: TimeInterval = 2.0) {
        retryAttempt = 0
        let delay = max(0, delaySeconds)
        installPendingTask(delaySeconds: delay)
        let roundedDelay = UInt32(max(0, ceil(delay)))
        snapshotValue = AppleBrowserStorageSyncSchedulerSnapshot(
            retryAttempt: 0,
            schedulerAction: "localChange",
            nextRunDelaySeconds: roundedDelay,
            nextRunAtUnixSeconds: appleBrowserStorageSyncSchedulerCurrentUnixSeconds() + Int64(roundedDelay),
            pendingRunScheduled: true,
            lastResultJson: snapshotValue.lastResultJson
        )
    }

    public func cancelPendingSync() {
        pendingTask?.cancel()
        pendingTask = nil
        snapshotValue.pendingRunScheduled = false
    }

    public func snapshot() -> AppleBrowserStorageSyncSchedulerSnapshot {
        snapshotValue
    }

    public func statusJson() -> String {
        appleBrowserStorageSyncSchedulerStatusJson(snapshotValue)
    }

    private func runScheduledSync() async {
        pendingTask = nil
        snapshotValue.pendingRunScheduled = false
        _ = await runCycle()
    }

    private func runCycle(
        generatedAtUnixSeconds: Int64? = nil,
        limitPerCollection: Int = 200,
        observedAtUnixSeconds: Int64? = nil
    ) async -> String {
        let attempt = retryAttempt
        let resultJson = await coordinator.runSyncCycle(
            generatedAtUnixSeconds: generatedAtUnixSeconds,
            limitPerCollection: limitPerCollection,
            retryAttempt: attempt,
            observedAtUnixSeconds: observedAtUnixSeconds
        )
        apply(resultJson: resultJson)
        return resultJson
    }

    private func apply(resultJson: String) {
        guard let result = appleBrowserStorageSyncSchedulerJsonObject(resultJson),
              let scheduleState = result["scheduleState"] as? [String: Any]
        else {
            retryAttempt = 0
            snapshotValue = AppleBrowserStorageSyncSchedulerSnapshot(
                lastResultJson: resultJson
            )
            return
        }

        let schedulerAction = appleBrowserStorageSyncSchedulerJsonString(scheduleState["schedulerAction"])
        let nextRetryAttempt = appleBrowserStorageSyncSchedulerUInt32(scheduleState["nextRetryAttempt"]) ?? 0
        let nextRunDelaySeconds = appleBrowserStorageSyncSchedulerUInt32(scheduleState["nextRunDelaySeconds"])
        let nextRunAtUnixSeconds = appleBrowserStorageSyncSchedulerInt64(scheduleState["nextRunAtUnixSeconds"])
        let waitingForCredentials = appleBrowserStorageSyncSchedulerJsonBool(scheduleState["waitingForCredentials"])
        let requiresUserAction = appleBrowserStorageSyncSchedulerJsonBool(scheduleState["requiresUserAction"])
        let resetRetryAttempt = appleBrowserStorageSyncSchedulerJsonBool(scheduleState["resetRetryAttempt"])

        retryAttempt = resetRetryAttempt ? 0 : nextRetryAttempt
        snapshotValue = AppleBrowserStorageSyncSchedulerSnapshot(
            retryAttempt: retryAttempt,
            schedulerAction: schedulerAction,
            nextRunDelaySeconds: nextRunDelaySeconds,
            nextRunAtUnixSeconds: nextRunAtUnixSeconds,
            waitingForCredentials: waitingForCredentials,
            requiresUserAction: requiresUserAction,
            pendingRunScheduled: false,
            lastResultJson: resultJson
        )

        if schedulesFollowUp,
           let delay = nextRunDelaySeconds,
           !waitingForCredentials {
            installPendingTask(delaySeconds: TimeInterval(delay))
            snapshotValue.pendingRunScheduled = true
        } else {
            pendingTask?.cancel()
            pendingTask = nil
        }
    }

    private func installPendingTask(delaySeconds: TimeInterval) {
        pendingTask?.cancel()
        let sleepNanoseconds = appleBrowserStorageSyncSchedulerSleepNanoseconds(delaySeconds)
        pendingTask = Task { [weak self] in
            if sleepNanoseconds > 0 {
                try? await Task.sleep(nanoseconds: sleepNanoseconds)
            }
            guard !Task.isCancelled else { return }
            await self?.runScheduledSync()
        }
    }
}

private func appleBrowserStorageSyncSchedulerJsonObject(_ json: String) -> [String: Any]? {
    guard json != "null",
          let data = json.data(using: .utf8)
    else {
        return nil
    }

    return try? JSONSerialization.jsonObject(with: data) as? [String: Any]
}

private func appleBrowserStorageSyncSchedulerStatusJson(_ snapshot: AppleBrowserStorageSyncSchedulerSnapshot) -> String {
    var payload: [String: Any] = [
        "retryAttempt": NSNumber(value: snapshot.retryAttempt),
        "schedulerAction": snapshot.schedulerAction,
        "waitingForCredentials": snapshot.waitingForCredentials,
        "requiresUserAction": snapshot.requiresUserAction,
        "pendingRunScheduled": snapshot.pendingRunScheduled,
    ]
    payload["nextRunDelaySeconds"] = snapshot.nextRunDelaySeconds.map { NSNumber(value: $0) } ?? NSNull()
    payload["nextRunAtUnixSeconds"] = snapshot.nextRunAtUnixSeconds.map { NSNumber(value: $0) } ?? NSNull()
    payload["lastResultJson"] = snapshot.lastResultJson ?? NSNull()

    guard JSONSerialization.isValidJSONObject(payload),
          let data = try? JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys]),
          let snapshotJson = String(data: data, encoding: .utf8)
    else {
        return "null"
    }

    return browserStorageSyncSchedulerStatusJson(schedulerSnapshotJson: snapshotJson)
}

private func appleBrowserStorageSyncSchedulerJsonString(_ value: Any?) -> String {
    value as? String ?? ""
}

private func appleBrowserStorageSyncSchedulerJsonBool(_ value: Any?) -> Bool {
    if let value = value as? Bool {
        return value
    }

    if let value = value as? NSNumber {
        return value.boolValue
    }

    return false
}

private func appleBrowserStorageSyncSchedulerUInt32(_ value: Any?) -> UInt32? {
    guard !(value is NSNull) else { return nil }

    if let value = value as? UInt32 {
        return value
    }

    if let value = value as? Int, value >= 0 {
        return UInt32(value)
    }

    if let value = value as? NSNumber {
        let uint64Value = value.uint64Value
        return uint64Value <= UInt64(UInt32.max) ? UInt32(uint64Value) : nil
    }

    return nil
}

private func appleBrowserStorageSyncSchedulerInt64(_ value: Any?) -> Int64? {
    guard !(value is NSNull) else { return nil }

    if let value = value as? Int64 {
        return value
    }

    if let value = value as? Int {
        return Int64(value)
    }

    if let value = value as? NSNumber {
        return value.int64Value
    }

    return nil
}

private func appleBrowserStorageSyncSchedulerSleepNanoseconds(_ seconds: TimeInterval) -> UInt64 {
    if seconds <= 0 {
        return 0
    }

    let nanoseconds = seconds * 1_000_000_000
    if nanoseconds >= TimeInterval(UInt64.max) {
        return UInt64.max
    }

    return UInt64(nanoseconds)
}

private func appleBrowserStorageSyncSchedulerCurrentUnixSeconds() -> Int64 {
    Int64(Date().timeIntervalSince1970)
}
