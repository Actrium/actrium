import type { NativeModule } from 'react-native';

import type { CapabilityManifestV1 } from './capability-verify';
import { contract, type ServiceEvent } from './contract';

export type { CapabilityManifestV1 };

export type ServiceEventHandler = (event: ServiceEvent) => void;

/** Source of service-to-front events for a single tab. */
export interface ServiceEventSource {
  /** The source must not call the handler after unsubscribe returns. */
  subscribe(handler: ServiceEventHandler): () => void;
}

export interface NativeServiceEventModule extends NativeModule {
  subscribeServiceEvents(tabId: string, events: string[]): Promise<string>;
  unsubscribeServiceEvents(tabId: string, subscriptionId: string): void;
}

type NativeServiceEventBody = ServiceEvent & {
  tabId: string;
  subscriptionId: string;
};

export type NativeServiceEventEmitter = {
  addListener(
    event: 'realmServiceEvent',
    listener: (event: unknown) => void
  ): { remove(): void };
};

function defaultNativeEventEmitter(
  nativeModule: NativeServiceEventModule
): NativeServiceEventEmitter {
  // Keep React Native's Flow sources out of non-RN unit-test module graphs.
  const { NativeEventEmitter } = require('react-native') as typeof import('react-native');
  return new NativeEventEmitter(nativeModule);
}

function parseServiceEvent(value: unknown): ServiceEvent {
  const parse = contract.hostModules['host:service'].on.schema?.parseEvent;
  if (!parse) throw new Error('The Service event schema is unavailable.');
  return parse(value);
}

function deliver(handlers: Iterable<ServiceEventHandler>, event: ServiceEvent): void {
  for (const handler of handlers) {
    try {
      handler(event);
    } catch (error) {
      // One Guest callback must not stop delivery to other callbacks or frames.
      console.warn('[Actrium] Service event handler failed:', error);
    }
  }
}

/** Create an event source backed by the RealmService RCTEventEmitter. */
export function createNativeServiceEventSource(
  nativeModule: NativeServiceEventModule | undefined,
  tabId: string,
  events: string[],
  createEmitter: (
    nativeModule: NativeServiceEventModule
  ) => NativeServiceEventEmitter = defaultNativeEventEmitter
): ServiceEventSource {
  const handlers = new Map<ServiceEventHandler, number>();
  let handlerCount = 0;
  const allowedEvents = new Set(events);
  let emitterSubscription: { remove(): void } | undefined;
  let nativeSubscriptionId: string | undefined;
  let generation = 0;
  // Native registers the subscription before its async subscribe reply returns.
  // Keep those early events by generation until the reply identifies which
  // native subscription they belong to. A finite buffer prevents a stale or
  // malicious emitter from growing memory while authorization is pending.
  const earlyEvents: {
    subscriptionId: string;
    event: ServiceEvent;
    handlers: ServiceEventHandler[];
  }[] = [];
  const maxEarlyEvents = 256;

  const stop = () => {
    generation += 1;
    earlyEvents.length = 0;
    emitterSubscription?.remove();
    emitterSubscription = undefined;
    if (nativeSubscriptionId) {
      nativeModule?.unsubscribeServiceEvents(tabId, nativeSubscriptionId);
      nativeSubscriptionId = undefined;
    }
  };

  const start = () => {
    if (!nativeModule?.subscribeServiceEvents || !nativeModule.unsubscribeServiceEvents) {
      throw new Error('The native Realm Service event bridge is unavailable.');
    }
    const current = ++generation;
    const emitter = createEmitter(nativeModule);
    emitterSubscription = emitter.addListener('realmServiceEvent', (value: unknown) => {
      if (current !== generation) return;
      const body = value as Partial<NativeServiceEventBody> | null;
      if (
        !body ||
        body.tabId !== tabId ||
        typeof body.subscriptionId !== 'string' ||
        typeof body.event !== 'string' ||
        !allowedEvents.has(body.event)
      ) {
        return;
      }
      // Once the reply is known, late events from an older native subscription
      // must not enter the current generation.
      if (nativeSubscriptionId && body.subscriptionId !== nativeSubscriptionId) return;
      let event: ServiceEvent;
      try {
        // The contract is the single authority for payload validation. Empty
        // base64 is valid and represents a zero-byte business payload.
        event = parseServiceEvent({
          event: body.event,
          payloadBase64: body.payloadBase64,
        });
      } catch (error) {
        console.warn('[Actrium] Ignoring a malformed native Service event:', error);
        return;
      }
      if (nativeSubscriptionId) {
        deliver([...handlers.keys()], event);
        return;
      }
      if (earlyEvents.length >= maxEarlyEvents) {
        console.warn('[Actrium] Service event subscription exceeded its early-event buffer.');
        handlers.clear();
        handlerCount = 0;
        stop();
        return;
      }
      earlyEvents.push({
        subscriptionId: body.subscriptionId,
        event,
        handlers: [...handlers.keys()],
      });
    });
    void nativeModule.subscribeServiceEvents(tabId, events).then(
      (subscriptionId) => {
        if (current !== generation) {
          nativeModule.unsubscribeServiceEvents(tabId, subscriptionId);
          return;
        }
        if (typeof subscriptionId !== 'string' || subscriptionId.length === 0) {
          console.warn('[Actrium] Service event subscription returned an invalid ID.');
          handlers.clear();
          handlerCount = 0;
          stop();
          return;
        }
        nativeSubscriptionId = subscriptionId;
        const pending = earlyEvents.splice(0);
        for (const item of pending) {
          if (current !== generation) break;
          if (item.subscriptionId === subscriptionId) {
            // Buffering is not replay for handlers added after the event. Also
            // exclude handlers that unsubscribed while the reply was pending.
            deliver(item.handlers.filter((handler) => handlers.has(handler)), item.event);
          }
        }
      },
      (error) => {
        if (current !== generation) return;
        console.warn('[Actrium] Service event subscription failed:', error);
        handlers.clear();
        handlerCount = 0;
        stop();
      }
    );
  };

  return {
    subscribe(handler) {
      const wasEmpty = handlerCount === 0;
      handlers.set(handler, (handlers.get(handler) ?? 0) + 1);
      handlerCount += 1;
      if (wasEmpty) {
        try {
          start();
        } catch (error) {
          const remaining = (handlers.get(handler) ?? 1) - 1;
          if (remaining === 0) handlers.delete(handler);
          else handlers.set(handler, remaining);
          handlerCount -= 1;
          stop();
          throw error;
        }
      }
      let active = true;
      const subscriptionGeneration = generation;
      return () => {
        if (!active) return;
        active = false;
        if (subscriptionGeneration !== generation) return;
        const remaining = (handlers.get(handler) ?? 0) - 1;
        if (remaining > 0) handlers.set(handler, remaining);
        else handlers.delete(handler);
        handlerCount = Math.max(0, handlerCount - 1);
        if (handlerCount === 0) stop();
      };
    },
  };
}

export interface HttpServiceEventSourceOptions {
  /** Maximum incomplete SSE frame size, measured in decoded UTF-16 units. */
  maxBufferedCharacters?: number;
  /** A connection this old is considered stable and resets reconnect backoff. */
  stableConnectionMs?: number;
  initialRetryDelayMs?: number;
  maxRetryDelayMs?: number;
}

type SseConnectionResult = {
  retry: boolean;
  error?: Error;
};

type ServiceEventXhr = {
  readyState: number;
  status: number;
  responseText: string;
  onreadystatechange: (() => void) | null;
  onprogress: (() => void) | null;
  onload: (() => void) | null;
  onerror: (() => void) | null;
  onabort: (() => void) | null;
  ontimeout: (() => void) | null;
  open(method: string, url: string, async: boolean): void;
  setRequestHeader(name: string, value: string): void;
  send(): void;
  abort(): void;
};

function asError(value: unknown): Error {
  return value instanceof Error ? value : new Error(String(value));
}

function retryableHttpStatus(status: number): boolean {
  // 204 is the SSE server's instruction to stop reconnecting. Other permanent
  // client errors will not improve without changing the request or credentials.
  if (status === 204) return false;
  if (status >= 400 && status < 500) return status === 408 || status === 429;
  return true;
}

/** React Native's whatwg-fetch resolves only after XHR `load`, which never
 * happens for a long-lived SSE response. Prefer incremental XHR on that host. */
function prefersIncrementalXhr(): boolean {
  return typeof navigator !== 'undefined' &&
    (navigator as Navigator & { product?: string }).product === 'ReactNative';
}

/**
 * Create an event source backed by an HTTP Server-Sent Events endpoint.
 *
 * Streaming fetch is used where available. React Native's whatwg-fetch does not
 * resolve until the response completes, and its Response has no body, so those
 * hosts use incremental XHR responseText instead. All handlers on this source
 * share one SSE connection.
 * Reconnects resume with Last-Event-ID when the server supplied an id; servers that do not persist IDs retain the documented
 * no-replay behavior.
 */
export function createHttpServiceEventSource(
  eventsUrl: string,
  events: string[],
  options: HttpServiceEventSourceOptions = {}
): ServiceEventSource {
  const maxBufferedCharacters = options.maxBufferedCharacters ?? 1_048_576;
  const stableConnectionMs = options.stableConnectionMs ?? 10_000;
  const initialRetryDelayMs = options.initialRetryDelayMs ?? 1_000;
  const maxRetryDelayMs = options.maxRetryDelayMs ?? 30_000;
  if (
    !Number.isFinite(maxBufferedCharacters) ||
    !Number.isFinite(stableConnectionMs) ||
    !Number.isFinite(initialRetryDelayMs) ||
    !Number.isFinite(maxRetryDelayMs) ||
    maxBufferedCharacters <= 0 ||
    stableConnectionMs < 0 ||
    initialRetryDelayMs < 0 ||
    maxRetryDelayMs < 0
  ) {
    throw new Error('Invalid Service event stream limits.');
  }

  const handlers = new Map<ServiceEventHandler, number>();
  let handlerCount = 0;
  let generation = 0;
  type ActiveStream = { stop(): void; readonly terminal: boolean };
  let activeStream: ActiveStream | undefined;

  const stop = () => {
    generation += 1;
    const stream = activeStream;
    activeStream = undefined;
    stream?.stop();
  };

  const start = () => {
      const url = new URL(eventsUrl, typeof location !== 'undefined' ? location.href : undefined);
      for (const event of events) url.searchParams.append('event', event);

      let active = true;
      let terminal = false;
      let retryCount = 0;
      let lastEventId: string | undefined;
      let cancelConnection: (() => void) | undefined;
      let retryTimer: ReturnType<typeof setTimeout> | undefined;
      let finishRetrySleep: (() => void) | undefined;

      const requestHeaders = (): Record<string, string> => ({
        Accept: 'text/event-stream',
        ...(lastEventId
          ? { 'Last-Event-ID': lastEventId }
          : {}),
      });

      const connect = () => {
        let settled = false;
        let buffer = '';
        let transportAbort: (() => void) | undefined;
        let stableTimer: ReturnType<typeof setTimeout> | undefined;
        let resolveDone!: (result: SseConnectionResult) => void;
        const done = new Promise<SseConnectionResult>((resolve) => {
          resolveDone = resolve;
        });

        const finish = (result: SseConnectionResult) => {
          if (settled) return;
          settled = true;
          if (!result.retry) terminal = true;
          if (stableTimer !== undefined) clearTimeout(stableTimer);
          transportAbort?.();
          transportAbort = undefined;
          resolveDone(result);
        };
        const markConnected = () => {
          if (stableTimer !== undefined) return;
          stableTimer = setTimeout(() => {
            retryCount = 0;
          }, stableConnectionMs);
        };
        const acceptText = (text: string) => {
          if (settled || !active) return;
          buffer += text;
          const chunks = buffer.split(/\r?\n\r?\n/);
          buffer = chunks.pop() ?? '';
          for (const chunk of chunks) {
            // A handler can synchronously close this connection and start a new
            // generation. Never deliver the rest of this old batch to its
            // replacement handlers or advance their resume state.
            if (settled || !active) break;
            const frame = parseSseChunk(chunk);
            if (frame.hasData && frame.id !== undefined) lastEventId = frame.id;
            if (!frame.eventName || !events.includes(frame.eventName)) continue;
            if (!frame.event) {
              console.warn(`[Actrium] Ignoring malformed Service event "${frame.eventName}".`);
              continue;
            }
            // A valid event proves the stream is making progress, even if the
            // server closes it before the stable-connection timer fires.
            retryCount = 0;
            deliver([...handlers.keys()], frame.event);
          }
          if (!settled && active && buffer.length > maxBufferedCharacters) {
            finish({
              retry: true,
              error: new Error(
                `Service event stream exceeded the ${maxBufferedCharacters}-character frame limit.`
              ),
            });
          }
        };
        const closed = () => {
          if (!settled) {
            finish({ retry: true, error: new Error('Service event stream closed.') });
          }
        };

        const startXhr = () => {
          if (settled) return;
          if (typeof XMLHttpRequest === 'undefined') {
            finish({
              retry: false,
              error: new Error(
                'Neither streaming fetch nor XMLHttpRequest is available for Service events.'
              ),
            });
            return;
          }
          const xhr = new XMLHttpRequest() as ServiceEventXhr;
          let offset = 0;
          transportAbort = () => xhr.abort();
          const readProgress = () => {
            if (settled || xhr.readyState < 3) return;
            if (xhr.status < 200 || xhr.status >= 300 || xhr.status === 204) {
              finish({
                retry: retryableHttpStatus(xhr.status),
                error: new Error(`Service event endpoint returned HTTP ${xhr.status}`),
              });
              return;
            }
            markConnected();
            const text = xhr.responseText ?? '';
            acceptText(text.slice(offset));
            offset = text.length;
          };
          xhr.onreadystatechange = () => {
            if (settled || xhr.readyState < 2) return;
            if (xhr.status < 200 || xhr.status >= 300 || xhr.status === 204) {
              finish({
                retry: retryableHttpStatus(xhr.status),
                error: new Error(`Service event endpoint returned HTTP ${xhr.status}`),
              });
            } else {
              markConnected();
            }
          };
          xhr.onprogress = readProgress;
          xhr.onload = () => {
            readProgress();
            closed();
          };
          xhr.onerror = () => finish({ retry: true, error: new Error('Service event XHR failed.') });
          xhr.ontimeout = () => finish({ retry: true, error: new Error('Service event XHR timed out.') });
          xhr.onabort = () => finish({ retry: true, error: new Error('Service event XHR aborted.') });
          try {
            xhr.open('GET', url.toString(), true);
            for (const [name, value] of Object.entries(requestHeaders())) {
              xhr.setRequestHeader(name, value);
            }
            xhr.send();
          } catch (error) {
            finish({ retry: false, error: asError(error) });
          }
        };

        if (typeof fetch !== 'function' || prefersIncrementalXhr()) {
          startXhr();
        } else {
          const controller = new AbortController();
          let switchedToXhr = false;
          transportAbort = () => controller.abort();
          void (async () => {
            const response = await fetch(url.toString(), {
              headers: requestHeaders(),
              signal: controller.signal,
            });
            if (settled) return;
            if (!response.ok || response.status === 204) {
              finish({
                retry: retryableHttpStatus(response.status),
                error: new Error(`Service event endpoint returned HTTP ${response.status}`),
              });
              return;
            }
            const reader = typeof response.body?.getReader === 'function'
              ? response.body.getReader()
              : undefined;
            if (!reader || typeof TextDecoder !== 'function') {
              // React Native fetch responses are not ReadableStreams. XHR is
              // the supported incremental transport on that host. Abort the
              // header-only fetch so it cannot remain as an orphan SSE request.
              switchedToXhr = true;
              reader?.releaseLock();
              controller.abort();
              startXhr();
              return;
            }
            markConnected();
            const decoder = new TextDecoder();
            try {
              while (!settled) {
                const { done: eof, value } = await reader.read();
                if (eof) break;
                acceptText(decoder.decode(value, { stream: true }));
              }
              if (!settled) acceptText(decoder.decode());
              closed();
            } finally {
              reader.releaseLock();
            }
          })().catch((error) => {
            if (!settled && !switchedToXhr) {
              finish({ retry: true, error: asError(error) });
            }
          });
        }

        cancelConnection = () => finish({ retry: false });
        return done;
      };

      const stream: ActiveStream = {
        get terminal() {
          return terminal;
        },
        stop: () => {
          if (!active) return;
          active = false;
          cancelConnection?.();
          cancelConnection = undefined;
          if (retryTimer !== undefined) clearTimeout(retryTimer);
          retryTimer = undefined;
          finishRetrySleep?.();
          finishRetrySleep = undefined;
        },
      };
      activeStream = stream;

      const consume = async () => {
        try {
          while (active) {
            const result = await connect();
            cancelConnection = undefined;
            if (!active) break;
            if (result.error) {
              if (result.retry) {
                console.warn('[Actrium] Service event stream failed:', result.error);
              } else {
                console.error('[Actrium] Service event stream failed:', result.error);
              }
            }
            if (!result.retry) break;

            const delayMs = Math.min(
              initialRetryDelayMs * 2 ** retryCount,
              maxRetryDelayMs
            );
            retryCount = Math.min(retryCount + 1, 30);
            await new Promise<void>((resolve) => {
              finishRetrySleep = resolve;
              retryTimer = setTimeout(resolve, delayMs);
            });
            retryTimer = undefined;
            finishRetrySleep = undefined;
          }
        } finally {
          // A stopped stream may already have a replacement. Only the stream
          // that is still registered can clear the shared active slot.
          if (activeStream === stream) activeStream = undefined;
        }
      };

      void consume();
  };

  return {
    subscribe(handler) {
      handlers.set(handler, (handlers.get(handler) ?? 0) + 1);
      handlerCount += 1;
      if (!activeStream || activeStream.terminal) {
        try {
          start();
        } catch (error) {
          const remaining = (handlers.get(handler) ?? 1) - 1;
          if (remaining > 0) handlers.set(handler, remaining);
          else handlers.delete(handler);
          handlerCount -= 1;
          if (handlerCount === 0) stop();
          throw error;
        }
      }
      let active = true;
      const subscriptionGeneration = generation;
      return () => {
        if (!active) return;
        active = false;
        if (subscriptionGeneration !== generation) return;
        const remaining = (handlers.get(handler) ?? 0) - 1;
        if (remaining > 0) handlers.set(handler, remaining);
        else handlers.delete(handler);
        handlerCount = Math.max(0, handlerCount - 1);
        if (handlerCount === 0) stop();
      };
    },
  };
}

/** Parse one SSE frame, retaining its resume ID even for filtered event names. */
function parseSseChunk(chunk: string): {
  id?: string;
  eventName?: string;
  hasData: boolean;
  event?: ServiceEvent;
} {
  let eventName: string | undefined;
  let id: string | undefined;
  const dataLines: string[] = [];

  for (const line of chunk.split(/\r?\n/)) {
    if (!line || line.startsWith(':')) continue;
    const colon = line.indexOf(':');
    const field = colon < 0 ? line : line.slice(0, colon);
    let value = colon < 0 ? '' : line.slice(colon + 1);
    if (value.startsWith(' ')) value = value.slice(1);
    if (field === 'event') eventName = value;
    else if (field === 'data') dataLines.push(value);
    else if (field === 'id' && !value.includes('\0')) id = value;
  }

  const hasData = dataLines.length > 0;
  if (!eventName || !hasData) return { id, eventName, hasData };
  try {
    const data = JSON.parse(dataLines.join('\n')) as { payloadBase64?: unknown };
    return {
      id,
      eventName,
      hasData,
      event: parseServiceEvent({ event: eventName, payloadBase64: data.payloadBase64 }),
    };
  } catch {
    return { id, eventName, hasData };
  }
}

/** Return the union of events declared by the canonical and legacy module keys. */
export function serviceEventsFromManifest(
  manifest: CapabilityManifestV1
): string[] {
  const events = new Set<string>();
  for (const key of ['service', 'host:service'] as const) {
    const declared = manifest.hostModules[key]?.events as unknown;
    if (declared === undefined) continue;
    // This function also runs on parsed signed JSON before TypeScript types
    // have been validated. Never iterate a string or another truthy value.
    if (!Array.isArray(declared) || declared.some((event) => typeof event !== 'string')) {
      throw new Error(`Capability manifest ${key}.events must be a string array.`);
    }
    for (const event of declared) events.add(event);
  }
  return [...events];
}
