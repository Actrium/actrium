import AVFoundation
import Foundation

private final class TestSynthesizer: AVSpeechSynthesizer {
    private(set) var utterance: AVSpeechUtterance?
    private(set) var stopCount = 0

    override func speak(_ utterance: AVSpeechUtterance) {
        self.utterance = utterance
    }

    override func stopSpeaking(at boundary: AVSpeechBoundary) -> Bool {
        stopCount += 1
        return true
    }
}

@main
@MainActor
private struct GuestTTSPlaybackRegression {
    static func main() async throws {
        try await stalledPlaybackFailsAndCleansUp()
        try await speechProgressExtendsDeadline()
        try failedReplacementStopsPreviousPlayback()
        print("Guest TTS playback regressions passed.")
    }

    private static func stalledPlaybackFailsAndCleansUp() async throws {
        let synthesizer = TestSynthesizer()
        let playback = GuestTTSPlayback(stallTimeout: .milliseconds(30), makeSynthesizer: { synthesizer })
        var results: [GuestTTSPlayback.Status] = []
        try playback.speak(sessionId: "guest", text: "No callbacks", locale: nil, voice: .system) {
            results.append($0)
        }
        let delegate = synthesizer.delegate
        let deadline = ContinuousClock.now + .seconds(5)
        while results.isEmpty && ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(10))
        }
        precondition(results == [.failed], "A missing callback must report one failure.")
        precondition(synthesizer.delegate == nil && synthesizer.stopCount == 1)

        delegate?.speechSynthesizer?(synthesizer, didFinish: synthesizer.utterance!)
        playback.stop(sessionId: "guest")
        await Task.yield()
        precondition(results == [.failed] && synthesizer.stopCount == 1, "A late callback must be ignored.")
    }

    private static func speechProgressExtendsDeadline() async throws {
        let synthesizer = TestSynthesizer()
        let playback = GuestTTSPlayback(stallTimeout: .milliseconds(500), makeSynthesizer: { synthesizer })
        var results: [GuestTTSPlayback.Status] = []
        try playback.speak(sessionId: "guest", text: "Still speaking", locale: nil, voice: .system) {
            results.append($0)
        }
        let utterance = synthesizer.utterance!
        synthesizer.delegate?.speechSynthesizer?(synthesizer, didStart: utterance)
        for _ in 0..<5 {
            try await Task.sleep(for: .milliseconds(120))
            synthesizer.delegate?.speechSynthesizer?(
                synthesizer, willSpeakRangeOfSpeechString: NSRange(location: 0, length: 5), utterance: utterance
            )
        }
        precondition(results.isEmpty, "Speech with progress must survive the first deadline.")
        synthesizer.delegate?.speechSynthesizer?(synthesizer, didFinish: utterance)
        let deadline = ContinuousClock.now + .seconds(5)
        while results.isEmpty && ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(10))
        }
        precondition(results == [.finished] && synthesizer.stopCount == 0)
    }

    private static func failedReplacementStopsPreviousPlayback() throws {
        let synthesizer = TestSynthesizer()
        let playback = GuestTTSPlayback(makeSynthesizer: { synthesizer })
        var results: [GuestTTSPlayback.Status] = []
        try playback.speak(sessionId: "guest", text: "Previous", locale: nil, voice: .system) {
            results.append($0)
        }
        do {
            try playback.speak(sessionId: "guest", text: "Replacement", locale: "zz-ZZ", voice: .male) { _ in
                preconditionFailure("A rejected replacement must not start.")
            }
            preconditionFailure("The requested voice should not be available.")
        } catch GuestTTSError.voiceUnavailable {
            precondition(results == [.cancelled] && synthesizer.stopCount == 1)
        }
    }
}
