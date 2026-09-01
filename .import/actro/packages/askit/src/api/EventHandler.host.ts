export type HandlerConfig<TPayload = any, TResult = any> = {
  responseEvent?: string;
  handle: (payload: TPayload) => TResult | Promise<TResult>;
};

export type HandlerRegistry = Record<string, HandlerConfig>;

type EngineLike = {
  on(event: 'message', listener: (message: { event: string; payload?: unknown }) => void): () => void;
  sendEvent(eventName: string, payload?: unknown): void;
};

type EventHandlerOptions = {
  tabId?: string;
  handlers?: HandlerRegistry;
  onLog?: (
    direction: 'guestToHost' | 'hostToGuest',
    event: string,
    payload: unknown,
    tabId?: string
  ) => void;
};

function buildErrorPayload(payload: unknown, error: unknown) {
  const requestId =
    payload && typeof payload === 'object' && 'requestId' in payload
      ? (payload as { requestId?: unknown }).requestId
      : undefined;

  return {
    requestId,
    success: false,
    message: error instanceof Error ? error.message : String(error),
  };
}

export const EventHandler = {
  setup(engine: EngineLike, options: EventHandlerOptions = {}): () => void {
    const { handlers = {}, onLog, tabId } = options;

    return engine.on('message', (message) => {
      const handler = handlers[message.event];
      if (!handler) return;

      onLog?.('guestToHost', message.event, message.payload, tabId);

      void Promise.resolve()
        .then(() => handler.handle(message.payload))
        .then((result) => {
          if (!handler.responseEvent) return;
          onLog?.('hostToGuest', handler.responseEvent, result, tabId);
          engine.sendEvent(handler.responseEvent, result);
        })
        .catch((error) => {
          if (!handler.responseEvent) return;
          const payload = buildErrorPayload(message.payload, error);
          onLog?.('hostToGuest', handler.responseEvent, payload, tabId);
          engine.sendEvent(handler.responseEvent, payload);
        });
    });
  },
};
