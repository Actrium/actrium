import Foundation
import React

@objc(GuestTTS)
final class GuestTTSModule: NSObject {
    @MainActor private lazy var playback = GuestTTSPlayback()

    @objc(speak:text:locale:voice:resolver:rejecter:)
    func speak(
        _ sessionId: String, text: String, locale: String?, voice: String,
        resolver resolve: @escaping RCTPromiseResolveBlock, rejecter reject: @escaping RCTPromiseRejectBlock
    ) {
        guard let preference = GuestTTSVoice(rawValue: voice) else {
            reject("invalid_voice", "Invalid text-to-speech voice preference.", nil)
            return
        }
        DispatchQueue.main.async {
            do {
                try self.playback.speak(sessionId: sessionId, text: text, locale: locale, voice: preference) {
                    resolve(["status": $0.rawValue])
                }
            } catch let error as GuestTTSError {
                reject(error.rawValue, "\(error.rawValue): \(error.localizedDescription)", error)
            } catch {
                reject("tts_failed", "Could not start text-to-speech playback.", error)
            }
        }
    }

    @objc(stop:resolver:rejecter:)
    func stop(
        _ sessionId: String, resolver resolve: @escaping RCTPromiseResolveBlock, rejecter _: @escaping RCTPromiseRejectBlock
    ) {
        DispatchQueue.main.async {
            self.playback.stop(sessionId: sessionId)
            resolve(nil)
        }
    }

    @objc static func requiresMainQueueSetup() -> Bool { false }
}
