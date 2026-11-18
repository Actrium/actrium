import type {
  ExpoSpeechRecognitionNativeEventMap,
  ExpoSpeechRecognitionOptions,
} from '@boomsi/rn-speech-recognition';
import type { SpeechInterimHandler, SpeechRecognizeInput, SpeechRecognizeOutput } from './contract';

type SpeechEvents = Pick<ExpoSpeechRecognitionNativeEventMap, 'start' | 'result' | 'error' | 'end'>;
type Subscription = { remove(): void };

export type SpeechRecognitionBackend = {
  requestPermissionsAsync(): Promise<{ granted: boolean }>;
  start(options: ExpoSpeechRecognitionOptions): void;
  stop(): void;
  abort(): void;
};

export type SpeechRecognitionEvents = {
  addListener<K extends keyof SpeechEvents>(name: K, listener: (event: SpeechEvents[K]) => void): Subscription;
};

const UTTERANCE_LIMIT_MS = 15_000;
const DICTATION_LIMIT_MS = 55_000;
const FINALIZATION_TIMEOUT_MS = 5_000;

type SpeechControls = {
  onStarted: (stop: () => void) => void;
  onFinalizing: () => void;
};

export function createSpeechRecognizer(backend: SpeechRecognitionBackend, events: SpeechRecognitionEvents) {
  let busy = false;
  const release = () => { busy = false; };

  return async (
    input: SpeechRecognizeInput, signal: AbortSignal, onInterim?: SpeechInterimHandler, controls?: SpeechControls
  ): Promise<SpeechRecognizeOutput> => {
    if (signal.aborted) throw cancelled();
    if (busy) throw new Error('Speech recognition is already in progress.');
    busy = true;

    try {
      const permission = await backend.requestPermissionsAsync();
      if (signal.aborted) throw cancelled();
      if (!permission.granted) throw new Error('Speech recognition requires system permission for speech and microphone access.');
    } catch (cause) {
      release();
      throw cause;
    }

    return new SpeechSession(backend, events, signal, release, onInterim, controls).run(input);
  };
}

function cancelled(): Error {
  return new Error('Speech recognition was cancelled.');
}

class SpeechSession {
  private readonly subscriptions: Subscription[] = [];
  private deadline?: ReturnType<typeof setTimeout>;
  private captureTimer?: ReturnType<typeof setTimeout>;
  private started = false;
  private stopping = false;
  private finished = false;
  private continuous = false;
  private nativeFailed = false;
  private abortRequested = false;
  private aborted = false;
  private text = '';
  private resolve!: (output: SpeechRecognizeOutput) => void;
  private reject!: (error: unknown) => void;

  constructor(
    private readonly backend: SpeechRecognitionBackend,
    private readonly events: SpeechRecognitionEvents,
    private readonly signal: AbortSignal,
    private readonly release: () => void,
    private readonly onInterim?: SpeechInterimHandler,
    private readonly controls?: SpeechControls,
  ) {}

  run({ locale, continuous = false }: SpeechRecognizeInput): Promise<SpeechRecognizeOutput> {
    this.continuous = continuous;
    return new Promise((resolve, reject) => {
      this.resolve = resolve;
      this.reject = reject;
      try {
        this.listen();
        this.signal.addEventListener('abort', this.cancel, { once: true });
        this.deadline = setTimeout(this.timeOut, this.captureLimit() + FINALIZATION_TIMEOUT_MS);
        this.backend.start({ lang: locale, continuous, maxAlternatives: 1, interimResults: this.onInterim !== undefined });
      } catch (cause) {
        this.cleanup();
        reject(cause);
      }
    });
  }

  private captureLimit(): number {
    return this.continuous ? DICTATION_LIMIT_MS : UTTERANCE_LIMIT_MS;
  }

  private listen(): void {
    this.subscriptions.push(this.events.addListener('start', this.onStart));
    this.subscriptions.push(this.events.addListener('result', this.onResult));
    this.subscriptions.push(this.events.addListener('error', ({ error, message }) => {
      this.nativeFailed = true;
      this.reject(new Error(`Speech recognition failed (${error}): ${message}`));
    }));
    this.subscriptions.push(this.events.addListener('end', () => {
      if (!this.started && !this.nativeFailed) return;
      this.cleanup();
      if (this.text) this.resolve({ text: this.text });
      else this.reject(new Error('No speech was recognized.'));
    }));
  }

  private onResult = ({ isFinal, results }: SpeechEvents['result']): void => {
    if (!this.started || this.abortRequested || this.nativeFailed) return;
    const transcript = results[0]?.transcript ?? '';
    // The native macOS module prefixes each segment after the first finalized segment with a space.
    const text = (this.continuous ? this.text + transcript : transcript).trim();
    if (isFinal) {
      if (text) this.text = text;
      if (!this.continuous) return;
    }
    this.deliver(() => this.onInterim?.({ text }));
  };

  private deliver(callback: () => void): void {
    try {
      callback();
    } catch (cause) {
      this.abortRequested = true;
      this.reject(new Error('Could not deliver the speech result to the Guest.', { cause }));
      this.abortNative();
    }
  }

  private onStart = (): void => {
    if (this.started) return;
    this.started = true;
    if (this.abortRequested) {
      this.abortNative();
      return;
    }
    this.captureTimer = setTimeout(this.stop, this.captureLimit());
    this.deliver(() => this.controls?.onStarted(this.stop));
  };

  private stop = (): void => {
    if (this.stopping || this.finished || this.abortRequested || this.nativeFailed) return;
    this.stopping = true;
    clearTimeout(this.captureTimer);
    try {
      this.backend.stop();
    } catch (cause) {
      this.reject(cause);
      this.abortNative();
      return;
    }
    if (!this.finished) this.deliver(() => this.controls?.onFinalizing());
  };

  private cancel = (): void => {
    this.abortRequested = true;
    this.reject(cancelled());
    // The native start schedules asynchronous work. Abort once it confirms capture.
    if (this.started) this.abortNative();
  };

  private timeOut = (): void => {
    this.abortRequested = true;
    this.reject(new Error('Speech recognition timed out.'));
    if (this.started || this.nativeFailed) this.abortNative();
  };

  private abortNative(): void {
    if (this.aborted) return;
    this.aborted = true;
    clearTimeout(this.captureTimer);
    try {
      this.backend.abort();
    } catch (cause) {
      this.reject(cause);
    }
  }

  private cleanup(): void {
    this.finished = true;
    clearTimeout(this.deadline);
    clearTimeout(this.captureTimer);
    this.signal.removeEventListener('abort', this.cancel);
    for (const subscription of this.subscriptions) subscription.remove();
    // The module emits unscoped events. A new Guest call may start only after end.
    this.release();
  }
}
