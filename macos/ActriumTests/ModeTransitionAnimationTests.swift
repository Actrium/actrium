import CoreFoundation
import Testing
@testable import Actrium

@MainActor
struct ModeTransitionAnimationTests {
    @Test func finalHandoffFadesContinuously() {
        let start = ModeTransitionConstants.handoffStartProgress
        #expect(ModeTransitionController.animationLayerOpacity(at: 0) == 1)
        #expect(ModeTransitionController.animationLayerOpacity(at: start) == 1)
        #expect(ModeTransitionController.animationLayerOpacity(at: 0.88) >
                ModeTransitionController.animationLayerOpacity(at: 0.94))
        #expect(ModeTransitionController.animationLayerOpacity(at: 0.94) > 0)
        #expect(ModeTransitionController.animationLayerOpacity(at: 1) == 0)
    }
}
