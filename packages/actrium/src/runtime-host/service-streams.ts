import { contract, type ServiceStreamChunk } from './contract';

export interface NativeServiceStreamModule {
  subscribeServiceStream(tabId: string, streamId: string): Promise<string>;
  unsubscribeServiceEvents(tabId: string, subscriptionId: string): void;
}

type StreamRegistration = { id?: string; ready: Promise<void> };

export class NativeServiceStreams {
  private readonly streams = new Map<string, StreamRegistration>();
  private disposed = false;

  constructor(
    private readonly tabId: string,
    private readonly native: NativeServiceStreamModule,
    private readonly listen: (listener: (event: unknown) => void) => () => void,
  ) {}

  async register(streamId: string): Promise<void> {
    if (this.disposed) throw new Error('The Service DataStream bridge is closed.');
    if (this.streams.has(streamId)) throw new Error('Service DataStream is already registered.');
    const registration: StreamRegistration = {
      ready: this.native.subscribeServiceStream(this.tabId, streamId).then((id) => {
        registration.id = id;
        if (this.streams.get(streamId) === registration) return;
        this.native.unsubscribeServiceEvents(this.tabId, id);
        throw new Error('Service DataStream registration was cancelled.');
      }),
    };
    this.streams.set(streamId, registration);
    try {
      await registration.ready;
    } catch (error) {
      if (this.streams.get(streamId) === registration) this.streams.delete(streamId);
      throw error;
    }
  }

  unregister(streamId: string): void {
    const registration = this.streams.get(streamId);
    if (!registration) return;
    this.streams.delete(streamId);
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
