import AVFoundation
import Foundation

enum GuestTTSVoice: String {
    case system
    case female
    case male

    func select(locale: String?) throws -> AVSpeechSynthesisVoice? {
        if self == .system {
            guard let locale else { return nil }
            guard let voice = AVSpeechSynthesisVoice(language: locale) else {
                throw GuestTTSError.voiceUnavailable
            }
            return voice
        }

        let language = locale ?? AVSpeechSynthesisVoice.currentLanguageCode()
        let voices = AVSpeechSynthesisVoice.speechVoices().filter {
            $0.language.caseInsensitiveCompare(language) == .orderedSame
                || $0.language.lowercased().hasPrefix(language.lowercased() + "-")
        }
        let gender: AVSpeechSynthesisVoiceGender = self == .female ? .female : .male
        if let voice = voices.first(where: { $0.gender == gender }) { return voice }

        // macOS reports the Mandarin Eloquence voices with unspecified gender.
        if self == .male,
           let voice = voices.first(where: { $0.identifier == "com.apple.eloquence.zh-CN.Grandpa" }) {
            return voice
        }
        throw GuestTTSError.voiceUnavailable
    }
}

enum GuestTTSError: String, LocalizedError {
    case voiceUnavailable = "voice_unavailable"

    var errorDescription: String? { "No matching system voice is available." }
}

@MainActor
final class GuestTTSPlayback {
    enum Status: String, Sendable {
        case finished
        case cancelled
        case failed
    }

    private enum Event: Sendable {
        case progress
        case completion(Status)
    }

    private struct Session {
        let token: UUID
        let synthesizer: AVSpeechSynthesizer
        let delegate: PlaybackDelegate
        let complete: (Status) -> Void
        var lastProgress: ContinuousClock.Instant
        let watchdog: Task<Void, Never>
    }

    private let stallTimeout: Duration
    private let makeSynthesizer: () -> AVSpeechSynthesizer
    private var sessions: [String: Session] = [:]

    init(stallTimeout: Duration = .seconds(30), makeSynthesizer: @escaping () -> AVSpeechSynthesizer = {
        AVSpeechSynthesizer()
    }) {
        self.stallTimeout = stallTimeout
        self.makeSynthesizer = makeSynthesizer
    }

    func speak(
        sessionId: String, text: String, locale: String?, voice: GuestTTSVoice,
        complete: @escaping (Status) -> Void
    ) throws {
        stop(sessionId: sessionId)
        let utterance = AVSpeechUtterance(string: text)
        utterance.voice = try voice.select(locale: locale)
        let synthesizer = makeSynthesizer()
        let token = UUID()
        let delegate = PlaybackDelegate { [weak self] event in
            self?.handle(event, sessionId: sessionId, token: token)
        }
        synthesizer.delegate = delegate
        let startedAt = ContinuousClock.now
        sessions[sessionId] = Session(
            token: token, synthesizer: synthesizer, delegate: delegate, complete: complete,
            lastProgress: startedAt, watchdog: watch(sessionId: sessionId, token: token, startedAt: startedAt)
        )
        synthesizer.speak(utterance)
    }

    func stop(sessionId: String) {
        guard let token = sessions[sessionId]?.token else { return }
        finish(sessionId: sessionId, token: token, status: .cancelled)
    }

    private func finish(sessionId: String, token: UUID, status: Status) {
        guard let session = sessions[sessionId], session.token == token else { return }
        sessions.removeValue(forKey: sessionId)
        session.watchdog.cancel()
        session.synthesizer.delegate = nil
        if status != .finished { session.synthesizer.stopSpeaking(at: .immediate) }
        session.complete(status)
    }

    private func handle(_ event: Event, sessionId: String, token: UUID) {
        switch event {
        case .progress:
            guard sessions[sessionId]?.token == token else { return }
            sessions[sessionId]?.lastProgress = .now
        case .completion(let status):
            finish(sessionId: sessionId, token: token, status: status)
        }
    }

    private func watch(sessionId: String, token: UUID, startedAt: ContinuousClock.Instant) -> Task<Void, Never> {
        // AVSpeechSynthesizerDelegate has no public failure callback.
        Task { [weak self, stallTimeout] in
            var deadline = startedAt + stallTimeout
            while !Task.isCancelled {
                do {
                    try await Task.sleep(until: deadline, clock: .continuous)
                } catch {
                    return  // The session cancelled its watchdog.
                }
                guard let self, let session = self.sessions[sessionId], session.token == token else { return }
                deadline = session.lastProgress + stallTimeout
                if deadline <= .now {
                    self.finish(sessionId: sessionId, token: token, status: .failed)
                    return
                }
            }
        }
    }

    private final class PlaybackDelegate: NSObject, AVSpeechSynthesizerDelegate {
        private let report: @MainActor @Sendable (Event) -> Void

        init(report: @escaping @MainActor @Sendable (Event) -> Void) {
            self.report = report
        }

        func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didStart utterance: AVSpeechUtterance) {
            notify(.progress)
        }

        func speechSynthesizer(
            _ synthesizer: AVSpeechSynthesizer, willSpeakRangeOfSpeechString range: NSRange, utterance: AVSpeechUtterance
        ) {
            notify(.progress)
        }

        func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didFinish utterance: AVSpeechUtterance) {
            notify(.completion(.finished))
        }

        func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didCancel utterance: AVSpeechUtterance) {
            notify(.completion(.cancelled))
        }

        private func notify(_ event: Event) {
            Task { @MainActor [report] in report(event) }
        }
    }
}
