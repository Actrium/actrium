import Foundation
import React
import AVFoundation

private let logger = AppLogger.reactNative

@objc(TTSModule)
class TTSModule: RCTEventEmitter {

    private let synthesizer = AVSpeechSynthesizer()
    private var currentUtterance: AVSpeechUtterance?

    override init() {
        super.init()
        synthesizer.delegate = self
        logger.notice("[TTSModule] Initialized")
    }

    deinit {
        logger.notice("[TTSModule] Deinitialized")
    }

    // MARK: - RCTEventEmitter

    override func supportedEvents() -> [String]! {
        return ["onTTSStart", "onTTSPause", "onTTSResume", "onTTSFinish", "onTTSError"]
    }

    @objc
    override static func requiresMainQueueSetup() -> Bool {
        return true
    }

    // MARK: - RN Methods

    @objc
    func speak(_ text: String, options: NSDictionary,
               resolve: @escaping RCTPromiseResolveBlock,
               reject: @escaping RCTPromiseRejectBlock) {
        logger.notice("[TTSModule] speak called with text: \(text), options: \(options)")

        DispatchQueue.main.async {
            // Switch audio session to playback mode for TTS
            // This ensures TTS plays at normal volume even after voice recording
            // which may have switched the session to .playAndRecord mode
            let audioSession = AVAudioSession.sharedInstance()
            do {
                try audioSession.setCategory(.playback, mode: .default, options: .duckOthers)
                try audioSession.setActive(true)
            } catch {
                logger.error("[TTSModule] Failed to set audio session category: \(error.localizedDescription)")
            }

            // Stop any current speech
            if self.synthesizer.isSpeaking {
                self.synthesizer.stopSpeaking(at: .immediate)
            }

            let utterance = AVSpeechUtterance(string: text)

            // Apply options
            if let rate = options["rate"] as? Float {
                utterance.rate = rate
            }
            if let pitchMultiplier = options["pitchMultiplier"] as? Float {
                utterance.pitchMultiplier = pitchMultiplier
            }
            if let volume = options["volume"] as? Float {
                utterance.volume = volume
            }

            // Set voice: voiceIdentifier takes priority, then language
            if let voiceIdentifier = options["voiceIdentifier"] as? String,
               let voice = AVSpeechSynthesisVoice(identifier: voiceIdentifier) {
                utterance.voice = voice
            } else if let language = options["language"] as? String,
                      let voice = AVSpeechSynthesisVoice(language: language) {
                utterance.voice = voice
            }

            self.currentUtterance = utterance
            self.synthesizer.speak(utterance)
            resolve(true)
        }
    }

    @objc
    func pause(_ resolve: @escaping RCTPromiseResolveBlock,
               reject: @escaping RCTPromiseRejectBlock) {
        logger.notice("[TTSModule] pause called")

        DispatchQueue.main.async {
            if self.synthesizer.isSpeaking {
                self.synthesizer.pauseSpeaking(at: .immediate)
                resolve(true)
            } else {
                resolve(false)
            }
        }
    }

    @objc
    func resume(_ resolve: @escaping RCTPromiseResolveBlock,
               reject: @escaping RCTPromiseRejectBlock) {
        logger.notice("[TTSModule] resume called")

        DispatchQueue.main.async {
            if self.synthesizer.isPaused {
                self.synthesizer.continueSpeaking()
                resolve(true)
            } else {
                resolve(false)
            }
        }
    }

    @objc
    func stop(_ resolve: @escaping RCTPromiseResolveBlock,
              reject: @escaping RCTPromiseRejectBlock) {
        logger.notice("[TTSModule] stop called")

        DispatchQueue.main.async {
            if self.synthesizer.isSpeaking || self.synthesizer.isPaused {
                self.synthesizer.stopSpeaking(at: .immediate)
                resolve(true)
            } else {
                resolve(false)
            }
        }
    }

    @objc
    func getAvailableLanguages(_ resolve: @escaping RCTPromiseResolveBlock,
                               reject: @escaping RCTPromiseRejectBlock) {
        logger.notice("[TTSModule] getAvailableLanguages called")

        let languages = AVSpeechSynthesisVoice.speechVoices().map { $0.language }
        let uniqueLanguages = Array(Set(languages)).sorted()
        resolve(uniqueLanguages)
    }
}

// MARK: - AVSpeechSynthesizerDelegate

extension TTSModule: AVSpeechSynthesizerDelegate {

    func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didStart utterance: AVSpeechUtterance) {
        logger.info("[TTSModule] Speech started")
        sendEvent(withName: "onTTSStart", body: nil)
    }

    func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didPause utterance: AVSpeechUtterance) {
        logger.info("[TTSModule] Speech paused")
        sendEvent(withName: "onTTSPause", body: nil)
    }

    func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didContinue utterance: AVSpeechUtterance) {
        logger.info("[TTSModule] Speech resumed")
        sendEvent(withName: "onTTSResume", body: nil)
    }

    func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didFinish utterance: AVSpeechUtterance) {
        logger.info("[TTSModule] Speech finished")
        currentUtterance = nil
        sendEvent(withName: "onTTSFinish", body: nil)
    }

    func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didCancel utterance: AVSpeechUtterance) {
        logger.info("[TTSModule] Speech cancelled")
        currentUtterance = nil
        // Cancel is treated as finish since it's intentional stop
        sendEvent(withName: "onTTSFinish", body: ["cancelled": true])
    }

    func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, error occurred: Error) {
        logger.error("[TTSModule] Speech error: \(occurred.localizedDescription)")
        sendEvent(withName: "onTTSError", body: ["message": occurred.localizedDescription])
    }
}
