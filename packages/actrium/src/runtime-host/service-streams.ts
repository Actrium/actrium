import { contract, type ServiceStreamChunk } from './contract';

export interface NativeServiceStreamModule {
  subscribeServiceStream(tabId: string, streamId: string): Promise<string>;
  unsubscribeServiceEvents(tabId: string, subscriptionId: string): void;
}

type StreamRegistration = { id?: string; stopWatching?: () => void };

export class NativeServiceStreams {
  private readonly streams = new Map<string, StreamRegistration>();
  private disposed = false;

  constructor(
    private readonly tabId: string,
    private readonly native: NativeServiceStreamModule,
    private readonly listen: (listener: (event: unknown) => void) => () => void,
  ) {}

  async register(
    streamId: string,
    watchRevocation?: (revoke: () => void) => (() => void) | undefined,
  ): Promise<void> {
    if (this.disposed) throw new Error('The Service DataStream bridge is closed.');
    if (this.streams.has(streamId)) throw new Error('Service DataStream is already registered.');
    const registration: StreamRegistration = {};
    this.streams.set(streamId, registration);
    try {
      const stopWatching = watchRevocation?.(() => this.unregisterRegistration(streamId, registration));
      if (this.streams.get(streamId) !== registration) {
        stopWatching?.();
        throw new Error('Service DataStream registration was cancelled.');
      }
      registration.stopWatching = stopWatching;
      const id = await this.native.subscribeServiceStream(this.tabId, streamId);
      if (this.streams.get(streamId) !== registration) {
        this.native.unsubscribeServiceEvents(this.tabId, id);
        throw new Error('Service DataStream registration was cancelled.');
      }
      registration.id = id;
    } catch (error) {
      this.unregisterRegistration(streamId, registration);
      throw error;
    }
  }

  unregister(streamId: string): void {
    const registration = this.streams.get(streamId);
    if (registration) this.unregisterRegistration(streamId, registration);
  }

  private unregisterRegistration(streamId: string, registration: StreamRegistration): void {
    if (this.streams.get(streamId) !== registration) return;
    this.streams.delete(streamId);
    registration.stopWatching?.();
    if (registration.id) this.native.unsubscribeServiceEvents(this.tabId, registration.id);
  }

  subscribe(handler: (chunk: ServiceStreamChunk) => void): () => void {
    return this.listen((value) => {
      if (!value || typeof value !== 'object') return;
      const event = value as Partial<ServiceStreamChunk> & { tabId?: string; subscriptionId?: string };
      if (event.tabId !== this.tabId || !event.streamId || !event.subscriptionId ||
          this.streams.get(event.streamId)?.id !== event.subscriptionId) return;
      const parse = contract.hostModules['host:service-stream'].onChunk.schema!.parseEvent!;
      handler(parse(event));
    });
  }

  dispose(): void {
    this.disposed = true;
    for (const streamId of this.streams.keys()) this.unregister(streamId);
  }
}
