import { NativeEventEmitter, NativeModules, type NativeModule } from 'react-native';
import { calculateNativeVolumeMetering } from '../AudioMetering';
import type {
  SpeechRecognitionCallbacks,
  SpeechRecognitionErrorEvent,
  SpeechRecognitionResultItem,
  SpeechRecognitionState,
  SpeechRecognitionTranscriptEvent,
  StartSpeechRecognitionOptions,
  Unsubscribe,
} from './index.types';

type SpeechRecognitionResultEvent = {
  isFinal: boolean;
  results: SpeechRecognitionResultItem[];
};

type SpeechRecognitionVolumeChangeEvent = {
  value?: number;
};

type ExpoSpeechRecognitionModuleLike = {
  start: (options: Record<string, unknown>) => void;
  stop: () => void;
  abort: () => void;
  requestPermissionsAsync: () => Promise<{ granted: boolean }>;
  addListener: (
    eventName: string,
    listener: (event: unknown) => void,
  ) => { remove: () => void };
};

const DEFAULT_OPTIONS = {
  lang: 'zh-CN',
  continuous: true,
  interimResults: true,
  addsPunctuation: true,
  maxAlternatives: 1,
};

/**
 * A lightweight adapter for expo-speech-recognition.
 *
 * The speech-recognition library now exposes a standard React Native native
 * module. Actro Runtime therefore reads the bridge from NativeModules instead
 * of relying on the earlier Expo Modules JavaScript wrapper.
 *
 * NativeModules and NativeEventEmitter provide the single integration path.
 */
export class SpeechRecognitionService {
  private state: SpeechRecognitionState = 'idle';
  private partialText = '';
  private finalText = '';
  private callbacks = new Set<SpeechRecognitionCallbacks>();
  private subscriptions: Array<{ remove: () => void }> = [];
  private initialized = false;
  private expoModule: ExpoSpeechRecognitionModuleLike | null = null;
  private availabilityChecked = false;
  private nativeEventEmitter: NativeEventEmitter | null = null;

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
    return this.getExpoModule() !== null;
  }

  subscribe(callbacks: SpeechRecognitionCallbacks): Unsubscribe {
    this.callbacks.add(callbacks);
    return () => {
      this.callbacks.delete(callbacks);
    };
  }

  async requestPermissions(): Promise<boolean> {
    const expoModule = this.getExpoModule();
    if (!expoModule) {
      this.emitUnavailableError();
      return false;
    }

    this.state = 'authorizing';
    const result = await expoModule.requestPermissionsAsync();

    if (!result.granted) {
      this.state = 'error';
      return false;
    }

    this.state = 'idle';
    return true;
  }

  async start(
    options: StartSpeechRecognitionOptions = {},
  ): Promise<boolean> {
    const expoModule = this.getExpoModule();
    if (!expoModule) {
      this.emitUnavailableError();
      return false;
    }

    this.ensureListeners();

    const granted = await this.requestPermissions();
    if (!granted) {
      return false;
    }

    this.resetTranscript();
    this.state = 'recognizing';

    expoModule.start({
      ...DEFAULT_OPTIONS,
      ...options,
    });

    return true;
  }

  stop(): void {
    const expoModule = this.getExpoModule();
    if (!expoModule) {
      return;
    }

    this.state = 'stopping';
    expoModule.stop();
  }

  abort(): void {
    const expoModule = this.getExpoModule();
    this.state = 'idle';
    this.resetTranscript();

    if (!expoModule) {
      return;
    }

    expoModule.abort();
  }

  dispose(): void {
    this.subscriptions.forEach((subscription) => subscription.remove());
    this.subscriptions = [];
    this.callbacks.clear();
    this.initialized = false;
  }

  private getExpoModule(): ExpoSpeechRecognitionModuleLike | null {
    if (this.availabilityChecked) {
      return this.expoModule;
    }

    this.availabilityChecked = true;

    try {
      const nativeModule = NativeModules.ExpoSpeechRecognition as
        | Omit<ExpoSpeechRecognitionModuleLike, 'addListener'>
        | undefined;

      if (!nativeModule) {
        return null;
      }

      // NativeEventEmitter owns subscriptions; method calls use the native module directly.
      this.nativeEventEmitter = new NativeEventEmitter(
        nativeModule as unknown as NativeModule,
      );
      this.expoModule = {
        ...nativeModule,
        addListener: (eventName, listener) => {
          return this.nativeEventEmitter!.addListener(eventName, listener);
        },
      };
      return this.expoModule;
    } catch (error) {
      console.warn(
        '[SpeechRecognitionService] speech recognition native module unavailable:',
        error,
      );
      this.expoModule = null;
      return null;
    }
  }

  private ensureListeners(): void {
    if (this.initialized) return;

    const expoModule = this.getExpoModule();
    if (!expoModule) {
      return;
    }

    this.subscriptions = [
      expoModule.addListener('start', () => {
        this.state = 'recognizing';
        this.callbacks.forEach((callbacks) => callbacks.onStart?.());
      }),
      expoModule.addListener('end', () => {
        this.state = 'idle';
        this.partialText = '';
        this.callbacks.forEach((callbacks) => callbacks.onEnd?.());
      }),
      expoModule.addListener('result', (rawEvent) => {
        const event = rawEvent as SpeechRecognitionResultEvent;
        const transcriptEvent = this.toTranscriptEvent(event);
        if (!transcriptEvent.text) return;

        if (event.isFinal) {
          this.finalText = transcriptEvent.text;
          this.partialText = '';
          this.callbacks.forEach((callbacks) =>
            callbacks.onFinalResult?.(transcriptEvent),
          );
          return;
        }

        this.partialText = transcriptEvent.text;
        this.callbacks.forEach((callbacks) =>
          callbacks.onPartialResult?.(transcriptEvent),
        );
      }),
      expoModule.addListener('volumechange', (rawEvent) => {
        const event = rawEvent as SpeechRecognitionVolumeChangeEvent;
        const metering = calculateNativeVolumeMetering(event.value ?? 0);
        this.callbacks.forEach((callbacks) => callbacks.onMetering?.(metering));
      }),
      expoModule.addListener('error', (rawError) => {
        const error = rawError as SpeechRecognitionErrorEvent;
        this.state = 'error';
        this.callbacks.forEach((callbacks) => callbacks.onError?.(error));
      }),
    ];

    this.initialized = true;
  }

  private emitUnavailableError(): void {
    this.state = 'error';
    const error: SpeechRecognitionErrorEvent = {
      error: 'service-not-allowed',
      message:
        'Speech recognition is unavailable because the ExpoSpeechRecognition native module is not registered or linked correctly.',
    };
    this.callbacks.forEach((callbacks) => callbacks.onError?.(error));
  }

  private resetTranscript(): void {
    this.partialText = '';
    this.finalText = '';
  }

  private toTranscriptEvent(
    event: SpeechRecognitionResultEvent,
  ): SpeechRecognitionTranscriptEvent {
    // Normalize on the first candidate so callers do not repeatedly inspect alternatives.
    const text = event.results[0]?.transcript?.trim() ?? '';
    return {
      text,
      isFinal: event.isFinal,
      results: event.results,
    };
  }
}

export const speechRecognitionService = new SpeechRecognitionService();
