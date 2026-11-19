import Foundation
import Testing
@testable import Actrium

struct RendererProcessPoolPolicyTests {
    @Test func processLimitScalesWithHardwareAndStaysBounded() {
        #expect(RendererProcessPool.recommendedProcessLimit(activeProcessorCount: 1) == 4)
        #expect(RendererProcessPool.recommendedProcessLimit(activeProcessorCount: 8) == 4)
        #expect(RendererProcessPool.recommendedProcessLimit(activeProcessorCount: 10) == 5)
        #expect(RendererProcessPool.recommendedProcessLimit(activeProcessorCount: 24) == 12)
        #expect(RendererProcessPool.recommendedProcessLimit(activeProcessorCount: 128) == 12)
    }

    @Test func systemApplicationsShareOneCanonicalRenderer() {
        #expect(
            RendererProcessPool.canonicalFingerprint("system.history")
                == "system.*"
        )
        #expect(
            RendererProcessPool.canShareRenderer(
                currentFingerprint: "system.*",
                requestedFingerprint: "system.bookmarks"
            )
        )
    }

    @Test func ordinaryApplicationsOnlyShareExactFingerprint() {
        #expect(
            RendererProcessPool.canShareRenderer(
                currentFingerprint: "actr.sha256:a",
                requestedFingerprint: "actr.sha256:a"
            )
        )
        #expect(
            !RendererProcessPool.canShareRenderer(
                currentFingerprint: "actr.sha256:a",
                requestedFingerprint: "actr.sha256:b"
            )
        )
        #expect(
            !RendererProcessPool.canShareRenderer(
                currentFingerprint: nil,
                requestedFingerprint: "actr.sha256:a"
            )
        )
    }

    @Test func navigationAcrossIdentityClassesRequiresAnotherRenderer() {
        #expect(
            !RendererProcessPool.canShareRenderer(
                currentFingerprint: "system.*",
                requestedFingerprint: "actr.sha256:a"
            )
        )
        #expect(
            !RendererProcessPool.canShareRenderer(
                currentFingerprint: "actr.sha256:a",
                requestedFingerprint: "system.history"
            )
        )
    }

    @Test func snapshotMapsTabsToOneSharedWorker() {
        let firstTab = UUID()
        let secondTab = UUID()
        let shared = RendererProcessSnapshot(
            id: "renderer-shared",
            pid: 42,
            fingerprint: "actr.sha256:a",
            tabIDs: [firstTab, secondTab],
            state: .ready
        )
        let snapshot = RendererProcessPoolSnapshot(processes: [shared])

        #expect(snapshot.processes.count == 1)
        #expect(snapshot.process(for: firstTab) == shared)
        #expect(snapshot.process(for: secondTab) == shared)
        #expect(snapshot.state(for: firstTab) == .ready)
        #expect(snapshot.process(for: UUID()) == nil)
        #expect(snapshot.state(for: UUID()) == nil)
    }

    @Test func snapshotPreservesStartingAndTerminatingStates() {
        let startingTab = UUID()
        let terminatingTab = UUID()
        let snapshot = RendererProcessPoolSnapshot(processes: [
            RendererProcessSnapshot(
                id: "renderer-starting",
                pid: 41,
                fingerprint: nil,
                tabIDs: [startingTab],
                state: .starting
            ),
            RendererProcessSnapshot(
                id: "renderer-terminating",
                pid: 42,
                fingerprint: "actr.sha256:a",
                tabIDs: [terminatingTab],
                state: .terminating
            ),
        ])

        #expect(snapshot.state(for: startingTab) == .starting)
        #expect(snapshot.state(for: terminatingTab) == .terminating)
        #expect(snapshot.process(for: terminatingTab)?.tabIDs == [terminatingTab])
    }

    @Test func attachmentResultReachesATabThatNeverReleasedItsRenderer() {
        // A tab that has never released a renderer has no stored epoch. The
        // captured epoch defaults to zero, so the first result must still be
        // delivered instead of being compared against nil.
        #expect(
            RendererProcessPool.attachmentResultIsCurrent(
                capturedEpoch: 0,
                currentEpoch: nil
            )
        )
        #expect(
            RendererProcessPool.attachmentResultIsCurrent(
                capturedEpoch: 0,
                currentEpoch: 0
            )
        )
    }

    @Test func attachmentResultIsDroppedAfterTheEpochAdvances() {
        #expect(
            !RendererProcessPool.attachmentResultIsCurrent(
                capturedEpoch: 0,
                currentEpoch: 1
            )
        )
        #expect(
            RendererProcessPool.attachmentResultIsCurrent(
                capturedEpoch: 2,
                currentEpoch: 2
            )
        )
        #expect(
            !RendererProcessPool.attachmentResultIsCurrent(
                capturedEpoch: 2,
                currentEpoch: 3
            )
        )
    }
}
