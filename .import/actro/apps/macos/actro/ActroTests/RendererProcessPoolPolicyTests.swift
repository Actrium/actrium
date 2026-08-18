import Testing
@testable import Actro

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
}
