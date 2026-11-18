import Foundation
import Testing
@testable import Actrium

private final class PIDMetricsReaderStub {
    var samples: [pid_t: PIDProcessSample] = [:]
    var failedPIDs: Set<pid_t> = []
    private(set) var readCounts: [pid_t: Int] = [:]

    func read(pid: pid_t) -> PIDProcessSample? {
        readCounts[pid, default: 0] += 1
        guard !failedPIDs.contains(pid) else { return nil }
        return samples[pid]
    }
}

struct PIDMetricsSamplerTests {
    private func makeSample(
        cpuTime: TimeInterval,
        identity: UInt64 = 1,
        memory: UInt64 = 4_096
    ) -> PIDProcessSample {
        PIDProcessSample(
            physicalFootprint: memory,
            cpuTime: cpuTime,
            processIdentity: identity
        )
    }

    @Test func firstSampleHasMemoryButNoCPUPercentage() {
        let reader = PIDMetricsReaderStub()
        let pid: pid_t = 42
        reader.samples[pid] = makeSample(cpuTime: 10)
        let now = 100.0
        let sampler = PIDMetricsSampler(
            reader: reader.read,
            monotonicClock: { now }
        )

        let metrics = sampler.sample(pids: [pid])[pid]

        #expect(metrics?.memoryBytes == 4_096)
        #expect(metrics?.cpuPercent == nil)
    }

    @Test func cpuPercentageUsesCPUDeltaOverMonotonicDelta() throws {
        let reader = PIDMetricsReaderStub()
        let singleCorePID: pid_t = 1
        let multiCorePID: pid_t = 2
        reader.samples[singleCorePID] = makeSample(cpuTime: 10)
        reader.samples[multiCorePID] = makeSample(cpuTime: 20)
        var now = 100.0
        let sampler = PIDMetricsSampler(
            reader: reader.read,
            monotonicClock: { now }
        )
        _ = sampler.sample(pids: [singleCorePID, multiCorePID])

        reader.samples[singleCorePID] = makeSample(cpuTime: 11)
        reader.samples[multiCorePID] = makeSample(cpuTime: 24)
        now = 102
        let metrics = sampler.sample(pids: [singleCorePID, multiCorePID])

        let singleCore = try #require(metrics[singleCorePID]?.cpuPercent)
        let multiCore = try #require(metrics[multiCorePID]?.cpuPercent)
        #expect(abs(singleCore - 50) < 0.000_001)
        #expect(abs(multiCore - 200) < 0.000_001)
    }

    @Test func zeroCPUDeltaIsReportedAsZero() throws {
        let reader = PIDMetricsReaderStub()
        let pid: pid_t = 42
        reader.samples[pid] = makeSample(cpuTime: 10)
        var now = 100.0
        let sampler = PIDMetricsSampler(
            reader: reader.read,
            monotonicClock: { now }
        )
        _ = sampler.sample(pids: [pid])

        now = 101
        let metrics = sampler.sample(pids: [pid])

        let cpuPercent = try #require(metrics[pid]?.cpuPercent)
        #expect(cpuPercent == 0)
    }

    @Test func sharedPIDIsReadOnlyOncePerSamplingRound() {
        let reader = PIDMetricsReaderStub()
        let pid: pid_t = 42
        reader.samples[pid] = makeSample(cpuTime: 10)
        var now = 100.0
        let sampler = PIDMetricsSampler(
            reader: reader.read,
            monotonicClock: { now }
        )

        _ = sampler.sample(pids: [pid, pid, pid])
        #expect(reader.readCounts[pid] == 1)

        now = 101
        _ = sampler.sample(pids: [pid, pid])
        #expect(reader.readCounts[pid] == 2)
    }

    @Test func failedReadRemovesMetricsAndResetsBaseline() throws {
        let reader = PIDMetricsReaderStub()
        let pid: pid_t = 42
        reader.samples[pid] = makeSample(cpuTime: 10)
        var now = 100.0
        let sampler = PIDMetricsSampler(
            reader: reader.read,
            monotonicClock: { now }
        )
        _ = sampler.sample(pids: [pid])

        reader.failedPIDs.insert(pid)
        now = 101
        let failed = sampler.sample(pids: [pid])[pid]
        #expect(failed?.memoryBytes == nil)
        #expect(failed?.cpuPercent == nil)

        reader.failedPIDs.remove(pid)
        reader.samples[pid] = makeSample(cpuTime: 10.5)
        now = 102
        #expect(sampler.sample(pids: [pid])[pid]?.cpuPercent == nil)

        reader.samples[pid] = makeSample(cpuTime: 11)
        now = 103
        let recovered = try #require(sampler.sample(pids: [pid])[pid]?.cpuPercent)
        #expect(abs(recovered - 50) < 0.000_001)
    }

    @Test func missingProcessResetsBaselineWhenPIDReturns() {
        let reader = PIDMetricsReaderStub()
        let pid: pid_t = 42
        reader.samples[pid] = makeSample(cpuTime: 10)
        var now = 100.0
        let sampler = PIDMetricsSampler(
            reader: reader.read,
            monotonicClock: { now }
        )
        _ = sampler.sample(pids: [pid])

        reader.samples.removeValue(forKey: pid)
        now = 101
        #expect(sampler.sample(pids: [pid])[pid]?.cpuPercent == nil)

        reader.samples[pid] = makeSample(cpuTime: 20, identity: 2)
        now = 102
        #expect(sampler.sample(pids: [pid])[pid]?.cpuPercent == nil)
    }

    @Test func backwardCPUCounterResetsBaseline() throws {
        let reader = PIDMetricsReaderStub()
        let pid: pid_t = 42
        reader.samples[pid] = makeSample(cpuTime: 10)
        var now = 100.0
        let sampler = PIDMetricsSampler(
            reader: reader.read,
            monotonicClock: { now }
        )
        _ = sampler.sample(pids: [pid])

        reader.samples[pid] = makeSample(cpuTime: 9)
        now = 101
        #expect(sampler.sample(pids: [pid])[pid]?.cpuPercent == nil)

        reader.samples[pid] = makeSample(cpuTime: 9.5)
        now = 102
        let recovered = try #require(sampler.sample(pids: [pid])[pid]?.cpuPercent)
        #expect(abs(recovered - 50) < 0.000_001)
    }

    @Test func changedProcessIdentityResetsBaseline() throws {
        let reader = PIDMetricsReaderStub()
        let pid: pid_t = 42
        reader.samples[pid] = makeSample(cpuTime: 10, identity: 1)
        var now = 100.0
        let sampler = PIDMetricsSampler(
            reader: reader.read,
            monotonicClock: { now }
        )
        _ = sampler.sample(pids: [pid])

        reader.samples[pid] = makeSample(cpuTime: 20, identity: 2)
        now = 101
        #expect(sampler.sample(pids: [pid])[pid]?.cpuPercent == nil)

        reader.samples[pid] = makeSample(cpuTime: 21, identity: 2)
        now = 102
        let recovered = try #require(sampler.sample(pids: [pid])[pid]?.cpuPercent)
        #expect(abs(recovered - 100) < 0.000_001)
    }
}
