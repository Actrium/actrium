import type { FrontCapability } from './capabilities';

export type TTSVoice = 'system' | 'female' | 'male';
export type TTSSpeakInput = { text: string; locale?: string; voice?: TTSVoice };
export type TTSSpeakOutput = { status: 'finished' | 'cancelled' };

export type GuestTTSBridge = {
  speak(sessionId: string, text: string, locale: string | null, voice: TTSVoice): Promise<TTSSpeakOutput>;
  stop(sessionId: string): Promise<void>;
};

type TTSOptions = {
  bridge: GuestTTSBridge;
  authorize: (capability: FrontCapability) => Promise<void>;
  onRevoked: (capability: FrontCapability, listener: () => void) => () => void;
  onCleanupError: (error: unknown) => void;
};

let sessionSequence = 0;

export class GuestTTS {
  private readonly sessionId = String(++sessionSequence);
  private cancelAuthorization?: () => void;
  private disposed = false;

  constructor(private readonly options: TTSOptions) {}

  private cancelPendingAuthorization() {
    const cancel = this.cancelAuthorization;
    this.cancelAuthorization = undefined;
    cancel?.();
  }

  private async cancel(): Promise<void> {
    this.cancelPendingAuthorization();
    return this.options.bridge.stop(this.sessionId);
  }

  async speak(input: TTSSpeakInput): Promise<TTSSpeakOutput> {
    if (this.disposed) throw new Error('The text-to-speech session has ended.');
    this.cancelPendingAuthorization();
    let interrupt!: () => void;
    const interrupted = new Promise<void>((resolve) => { interrupt = resolve; });
    this.cancelAuthorization = interrupt;
    try {
      await Promise.race([this.options.authorize('host:tts.speak'), interrupted]);
      if (this.cancelAuthorization !== interrupt) return { status: 'cancelled' };
    } finally {
      if (this.cancelAuthorization === interrupt) this.cancelAuthorization = undefined;
    }

    let revoked = false;
    const unsubscribe = this.options.onRevoked('host:tts.speak', () => {
      revoked = true;
      void this.cancel().catch(this.options.onCleanupError);
    });
    try {
      if (revoked) return { status: 'cancelled' };
      return await this.options.bridge.speak(
        this.sessionId, input.text, input.locale ?? null, input.voice ?? 'system'
      );
    } finally {
      unsubscribe();
    }
  }

  async stop(): Promise<void> {
    await this.options.authorize('host:tts.stop');
    return this.cancel();
  }

  async dispose(): Promise<void> {
    if (this.disposed) return;
    this.disposed = true;
    return this.cancel();
  }
}
