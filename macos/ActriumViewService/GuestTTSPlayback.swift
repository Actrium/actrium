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
    }

    private struct Session {
        let token: UUID
        let synthesizer: AVSpeechSynthesizer
        let delegate: PlaybackDelegate
        let complete: (Status) -> Void
    }

    private var sessions: [String: Session] = [:]

    func speak(
        sessionId: String, text: String, locale: String?, voice: GuestTTSVoice,
        complete: @escaping (Status) -> Void
    ) throws {
        let utterance = AVSpeechUtterance(string: text)
        utterance.voice = try voice.select(locale: locale)
        stop(sessionId: sessionId)
        let synthesizer = AVSpeechSynthesizer()
        let token = UUID()
        let delegate = PlaybackDelegate { [weak self] status in
            self?.finish(sessionId: sessionId, token: token, status: status)
        }
        synthesizer.delegate = delegate
        sessions[sessionId] = Session(token: token, synthesizer: synthesizer, delegate: delegate, complete: complete)
        synthesizer.speak(utterance)
    }

    func stop(sessionId: String) {
        guard let session = sessions.removeValue(forKey: sessionId) else { return }
        session.synthesizer.delegate = nil
        session.synthesizer.stopSpeaking(at: .immediate)
        session.complete(.cancelled)
    }

    private func finish(sessionId: String, token: UUID, status: Status) {
        guard sessions[sessionId]?.token == token else { return }
        sessions.removeValue(forKey: sessionId)?.complete(status)
    }

    private final class PlaybackDelegate: NSObject, AVSpeechSynthesizerDelegate {
        private let complete: @MainActor @Sendable (Status) -> Void

        init(complete: @escaping @MainActor @Sendable (Status) -> Void) {
            self.complete = complete
        }

        func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didFinish utterance: AVSpeechUtterance) {
            notify(.finished)
        }

        func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didCancel utterance: AVSpeechUtterance) {
            notify(.cancelled)
        }

        private func notify(_ status: Status) {
            Task { @MainActor [complete] in complete(status) }
        }
    }
}
