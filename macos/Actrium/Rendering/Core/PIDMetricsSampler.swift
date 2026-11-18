import Darwin
import Foundation

struct PIDProcessSample: Equatable, Sendable {
    let physicalFootprint: UInt64
    let cpuTime: TimeInterval
    let processIdentity: UInt64
}

struct PIDMetrics: Equatable, Sendable {
    let memoryBytes: UInt64?
    let cpuPercent: Double?
}

final class PIDMetricsSampler {
    typealias Reader = (pid_t) -> PIDProcessSample?
    typealias MonotonicClock = () -> TimeInterval

    private struct Baseline {
        let cpuTime: TimeInterval
        let monotonicTime: TimeInterval
        let processIdentity: UInt64
    }

    private let reader: Reader
    private let monotonicClock: MonotonicClock
    private var baselines: [pid_t: Baseline] = [:]

    init(
        reader: @escaping Reader = PIDMetricsSampler.readProcessSample,
        monotonicClock: @escaping MonotonicClock = {
            ProcessInfo.processInfo.systemUptime
        }
    ) {
        self.reader = reader
        self.monotonicClock = monotonicClock
    }

    func sample(pids: [pid_t]) -> [pid_t: PIDMetrics] {
        let uniquePIDs = Set(pids)
        baselines = baselines.filter { uniquePIDs.contains($0.key) }

        let monotonicTime = monotonicClock()
        var metrics: [pid_t: PIDMetrics] = [:]
        for pid in uniquePIDs.sorted() {
            guard let sample = reader(pid) else {
                baselines.removeValue(forKey: pid)
                metrics[pid] = PIDMetrics(memoryBytes: nil, cpuPercent: nil)
                continue
            }

            let baseline = Baseline(
                cpuTime: sample.cpuTime,
                monotonicTime: monotonicTime,
                processIdentity: sample.processIdentity
            )
            defer { baselines[pid] = baseline }

            guard let previous = baselines[pid],
                  previous.processIdentity == sample.processIdentity,
                  sample.cpuTime >= previous.cpuTime,
                  monotonicTime > previous.monotonicTime else {
                metrics[pid] = PIDMetrics(
                    memoryBytes: sample.physicalFootprint,
                    cpuPercent: nil
                )
                continue
            }

            let cpuDelta = sample.cpuTime - previous.cpuTime
            let timeDelta = monotonicTime - previous.monotonicTime
            metrics[pid] = PIDMetrics(
                memoryBytes: sample.physicalFootprint,
                cpuPercent: 100 * cpuDelta / timeDelta
            )
        }
        return metrics
    }

    static func readProcessSample(pid: pid_t) -> PIDProcessSample? {
        var info = rusage_info_current()
        let result = withUnsafeMutablePointer(to: &info) { pointer in
            pointer.withMemoryRebound(to: rusage_info_t?.self, capacity: 1) { rusagePointer in
                proc_pid_rusage(pid, RUSAGE_INFO_CURRENT, rusagePointer)
            }
        }
        guard result == 0 else { return nil }

        let userSeconds = TimeInterval(info.ri_user_time) / 1_000_000_000
        let systemSeconds = TimeInterval(info.ri_system_time) / 1_000_000_000
        return PIDProcessSample(
            physicalFootprint: info.ri_phys_footprint,
            cpuTime: userSeconds + systemSeconds,
            processIdentity: info.ri_proc_start_abstime
        )
    }
}
