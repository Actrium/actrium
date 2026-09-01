import {
  NativeEventEmitter,
  NativeModules,
  PermissionsAndroid,
  Platform,
} from 'react-native';
import { createAudioStream, type AudioStreamManager } from '../AudioStreamManager';
import { calculatePcmMetering } from '../AudioMetering';
import { Utils } from '../../../utils/utils';
import { ASR_BRIDGE_CONTRACT } from '../../coreContract';
import {
  feedAudio,
  setLanguage as setSpeechLanguage,
  stop as stopSpeechRecognition,
} from '../../../native-bridge/speech/SpeechBridge';
import type {
  SpeechRecognitionCallbacks,
  SpeechRecognitionErrorEvent,
  SpeechRecognitionResultItem,
  SpeechRecognitionState,
  SpeechRecognitionTranscriptEvent,
  StartSpeechRecognitionOptions,
  Unsubscribe,
} from './index.types';

type AndroidAsrResultEvent =
  | string
  | {
      text?: string;
      transcript?: string;
      result?: string;
      isFinal?: boolean;
      final?: boolean;
      confidence?: number;
    };

const DEFAULT_OPTIONS = {
  lang: 'zh-CN',
  continuous: true,
  interimResults: true,
  addsPunctuation: true,
  maxAlternatives: 1,
};

const asrModule = NativeModules[ASR_BRIDGE_CONTRACT.moduleName];

export class SpeechRecognitionService {
  private state: SpeechRecognitionState = 'idle';
  private partialText = '';
  private finalText = '';
  private callbacks = new Set<SpeechRecognitionCallbacks>();
  private eventEmitter: NativeEventEmitter | null = null;
  private subscriptions: Array<{ remove: () => void }> = [];
  private listenersInitialized = false;
  private audioStream: AudioStreamManager | null = null;
  private startSessionId = 0;
  private activeSessionId: number | null = null;
  private isAborting = false;

  getState(): SpeechRecognitionState {
    return this.state;
  }

  getCurrentTranscript(): { partialText: string; finalText: string } {
    return {
      partialText: this.partialText,
      finalText: this.finalText,
    };
  }

  isAvailable(): boolean {
    return !!asrModule;
  }

  subscribe(callbacks: SpeechRecognitionCallbacks): Unsubscribe {
    this.callbacks.add(callbacks);
    return () => {
      this.callbacks.delete(callbacks);
    };
  }

  async requestPermissions(): Promise<boolean> {
    if (!this.isAvailable()) {
      this.emitUnavailableError();
      return false;
    }

    this.state = 'authorizing';
    if (Platform.OS !== 'android') {
      this.state = 'idle';
      return true;
    }

    try {
      const granted = await PermissionsAndroid.request(
        PermissionsAndroid.PERMISSIONS.RECORD_AUDIO,
        {
          title: 'Microphone Permissions',
          message: 'We need to use the microphone for voice recognition.',
          buttonPositive: 'Allow',
          buttonNegative: 'Reject',
        },
      );

      if (granted === PermissionsAndroid.RESULTS.GRANTED) {
        this.state = 'idle';
        return true;
      }

      this.state = 'error';
      return false;
    } catch (error) {
      this.state = 'error';
      this.emitError({
        error: 'permission-request-failed',
        message: error instanceof Error ? error.message : String(error),
      });
      return false;
    }
  }

  async start(
    options: StartSpeechRecognitionOptions = {},
  ): Promise<boolean> {
    if (!this.isAvailable()) {
      this.emitUnavailableError();
      return false;
    }

    this.ensureListeners();

    const granted = await this.requestPermissions();
    if (!granted) {
      return false;
    }

    this.startSessionId += 1;
    this.activeSessionId = this.startSessionId;
    this.isAborting = false;
    this.resetTranscript();
    this.state = 'recognizing';

    try {
      await setSpeechLanguage(options.lang ?? DEFAULT_OPTIONS.lang);
      this.ensureAudioStream();
      await this.audioStream!.start();
      this.callbacks.forEach((callbacks) => callbacks.onStart?.());
      return true;
    } catch (error) {
      console.error('[SpeechRecognitionService.android] start failed:', error);
      this.stopAudioStream();
      this.state = 'error';
      this.emitError({
        error: 'start-failed',
        message: error instanceof Error ? error.message : String(error),
      });
      return false;
    }
  }

  stop(): void {
    if (this.activeSessionId == null) {
      return;
    }

    this.state = 'stopping';
    const currentSessionId = this.activeSessionId;
    this.stopAudioStream();
    void this.finalizeRecognition(currentSessionId, false);
  }

  abort(): void {
    if (this.activeSessionId == null) {
      this.state = 'idle';
      this.resetTranscript();
      return;
    }

    this.isAborting = true;
    const currentSessionId = this.activeSessionId;
    this.stopAudioStream();
    this.resetTranscript();
    this.state = 'idle';
    void this.finalizeRecognition(currentSessionId, true);
  }

  dispose(): void {
    this.stopAudioStream();
    this.subscriptions.forEach((subscription) => subscription.remove());
    this.subscriptions = [];
    this.listenersInitialized = false;
    this.callbacks.clear();
  }

  private ensureAudioStream(): void {
    if (this.audioStream) {
      return;
    }

    this.audioStream = createAudioStream((chunk) => {
      if (this.state !== 'recognizing') {
        return;
      }

      // Re-assigned for Send ASR The same one. PCM Slice to calculate the volume and avoid additional microphone flow.
      const metering = calculatePcmMetering(chunk);
      this.callbacks.forEach((callbacks) => callbacks.onMetering?.(metering));

      // Andre. ASR Need to feed on a continuous basis base64 PCM Data, here. Uint8Array Harmonized format for the return of original bridge requirements.
      const base64Audio = Utils.uint8ArrayToBase64(chunk);
      void feedAudio(base64Audio).catch((error) => {
        console.error('[SpeechRecognitionService.android] feedAudio failed:', error);
        this.state = 'error';
        this.emitError({
          error: 'feed-audio-failed',
          message: error instanceof Error ? error.message : String(error),
        });
      });
    }, (error) => {
      this.state = 'error';
      this.emitError({
        error: 'audio-stream-failed',
        message: error.message,
      });
    });
  }

  private stopAudioStream(): void {
    if (!this.audioStream) {
      return;
    }
    this.audioStream.stop();
  }

  private ensureListeners(): void {
    if (this.listenersInitialized || !this.isAvailable()) {
      return;
    }

    this.eventEmitter = new NativeEventEmitter(asrModule);
    this.subscriptions = [
      this.eventEmitter.addListener(
        ASR_BRIDGE_CONTRACT.resultEventName,
        (event: AndroidAsrResultEvent) => {
          const transcriptEvent = this.toTranscriptEvent(event, false);
          if (!transcriptEvent.text) {
            return;
          }

          this.partialText = transcriptEvent.text;
          this.callbacks.forEach((callbacks) =>
            callbacks.onPartialResult?.(transcriptEvent),
          );
        },
      ),
      this.eventEmitter.addListener(
        ASR_BRIDGE_CONTRACT.errorEventName,
        (rawError: unknown) => {
          const error = this.toErrorEvent(rawError);
          this.state = 'error';
          this.callbacks.forEach((callbacks) => callbacks.onError?.(error));
        },
      ),
    ];

    this.listenersInitialized = true;
  }

  private async finalizeRecognition(
    sessionId: number,
    ignoreResult: boolean,
  ): Promise<void> {
    try {
      const finalText = await stopSpeechRecognition();
      if (this.activeSessionId !== sessionId) {
        return;
      }

      this.activeSessionId = null;
      if (!ignoreResult && finalText?.trim()) {
        const transcriptEvent = this.toTranscriptEvent(finalText, true);
        this.finalText = transcriptEvent.text;
        this.partialText = '';
        this.callbacks.forEach((callbacks) =>
          callbacks.onFinalResult?.(transcriptEvent),
        );
      } else {
        this.resetTranscript();
      }
    } catch (error) {
      if (this.activeSessionId === sessionId) {
        this.activeSessionId = null;
      }
      if (!ignoreResult) {
        this.state = 'error';
        this.emitError({
          error: 'stop-failed',
          message: error instanceof Error ? error.message : String(error),
        });
      }
    } finally {
      if (this.activeSessionId === null) {
        this.state = 'idle';
        this.isAborting = false;
        this.callbacks.forEach((callbacks) => callbacks.onEnd?.());
      }
    }
  }

  private emitUnavailableError(): void {
    this.emitError({
      error: 'service-not-allowed',
      message: `speech recognition Current not available: native module ${ASR_BRIDGE_CONTRACT.moduleName} Can not get folder: %s: %s`,
    });
  }

  private emitError(error: SpeechRecognitionErrorEvent): void {
    this.callbacks.forEach((callbacks) => callbacks.onError?.(error));
  }

  private resetTranscript(): void {
    this.partialText = '';
    this.finalText = '';
  }

  private toTranscriptEvent(
    event: AndroidAsrResultEvent,
    isFinalFallback: boolean,
  ): SpeechRecognitionTranscriptEvent {
    const normalized = this.normalizeResultEvent(event);
    return {
      text: normalized.text,
      isFinal: normalized.isFinal ?? isFinalFallback,
      results: [
        {
          transcript: normalized.text,
          confidence: normalized.confidence ?? 0,
          segments: [],
        },
      ],
    };
  }

  private normalizeResultEvent(
    event: AndroidAsrResultEvent,
  ): {
    text: string;
    isFinal?: boolean;
    confidence?: number;
  } {
    if (typeof event === 'string') {
      return { text: event.trim() };
    }

    return {
      text: (
        event[ASR_BRIDGE_CONTRACT.keys.text] ??
        event.transcript ??
        event.result ??
        ''
      ).trim(),
      isFinal: event[ASR_BRIDGE_CONTRACT.keys.isFinal] ?? event.final,
      confidence: event.confidence,
    };
  }

  private toErrorEvent(rawError: unknown): SpeechRecognitionErrorEvent {
    if (typeof rawError === 'string') {
      return {
        error: 'native-error',
        message: rawError,
      };
    }

    if (rawError && typeof rawError === 'object') {
      const maybeError = rawError as {
        error?: string;
        message?: string;
        code?: number;
      };
      return {
        error: maybeError.error ?? 'native-error',
        message: maybeError.message ?? 'Android ASR error',
        code: maybeError.code,
      };
    }

    return {
      error: 'native-error',
      message: 'Android ASR error',
    };
  }
}

export const speechRecognitionService = new SpeechRecognitionService();
