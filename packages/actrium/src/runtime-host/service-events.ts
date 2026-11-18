import type { NativeModule } from 'react-native';

import type { CapabilityManifestV1 } from './capability-verify';
import type { ServiceEvent } from './contract';

export type { CapabilityManifestV1 };

export type ServiceEventHandler = (event: ServiceEvent) => void;

/**
 * Source of service-to-front events for a single tab.
 *
 * The source is constructed with the set of event names the package is allowed
 * to receive (from its capability manifest). Subscribers registered through
 * `subscribe` receive every event that the source delivers; guest-side SDK
 * wrappers are responsible for filtering by `event.event` when the guest only
 * asked for a specific name.
 */
export interface ServiceEventSource {
  /**
   * Register a handler for service events. Returns an unsubscribe function.
   * The source must not call the handler after unsubscribe returns.
   */
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

/**
 * Create an event source backed by the RealmService RCTEventEmitter.
 *
 * All Guest handlers for a tab share one native actor subscription. The last
 * unsubscribe removes both the NativeEventEmitter listener and the Host-side
 * actor subscription. Events are additionally scoped by tab/subscription ID and
 * the signed manifest's event allowlist.
 */
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

  const stop = () => {
    generation += 1;
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
        body.subscriptionId !== nativeSubscriptionId ||
        typeof body.event !== 'string' ||
        !allowedEvents.has(body.event) ||
        typeof body.payloadBase64 !== 'string' ||
        body.payloadBase64.length === 0
      ) {
        return;
      }
      const event = { event: body.event, payloadBase64: body.payloadBase64 };
      for (const handler of [...handlers.keys()]) {
        try {
          handler(event);
        } catch (error) {
          console.warn('[Actrium] Service event handler failed:', error);
        }
      }
    });
    void nativeModule.subscribeServiceEvents(tabId, events).then(
      (subscriptionId) => {
        if (current !== generation) {
          nativeModule.unsubscribeServiceEvents(tabId, subscriptionId);
          return;
        }
        nativeSubscriptionId = subscriptionId;
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
          throw error;
        }
      }
      let active = true;
      return () => {
        if (!active) return;
        active = false;
        const remaining = (handlers.get(handler) ?? 0) - 1;
        if (remaining > 0) handlers.set(handler, remaining);
        else handlers.delete(handler);
        handlerCount = Math.max(0, handlerCount - 1);
        if (handlerCount === 0) stop();
      };
    },
  };
}

/**
 * Create a test event source that allows tests to inject events manually.
 */
export function createTestServiceEventSource(events: string[]): {
  source: ServiceEventSource;
  emit(event: ServiceEvent): void;
} {
  const handlers = new Set<ServiceEventHandler>();
  return {
    source: {
      subscribe(handler) {
        handlers.add(handler);
        return () => {
          handlers.delete(handler);
        };
      },
    },
    emit(event) {
      if (!events.includes(event.event)) {
        throw new Error(
          `Event "${event.event}" is not in the allowed test event set.`
        );
      }
      for (const handler of handlers) {
        handler(event);
      }
    },
  };
}

/**
 * Create an event source backed by an HTTP Server-Sent Events endpoint.
 *
 * The source fetches `eventsUrl` and parses the `text/event-stream` body. Each
 * SSE event's `event:` field names the event; the `data:` field must be a JSON
 * object with a `payloadBase64` string. This uses `fetch` with a streaming
 * response body instead of `EventSource`, so it also works in React Native
 * environments that expose `response.body`.
 */
export function createHttpServiceEventSource(
  eventsUrl: string,
  events: string[]
): ServiceEventSource {
  return {
    subscribe(handler) {
      const url = new URL(eventsUrl, typeof location !== 'undefined' ? location.href : undefined);
      for (const event of events) {
        url.searchParams.append('event', event);
      }

      const controller = new AbortController();
      let active = true;
      let retryCount = 0;

      const consume = async () => {
        while (active) {
          try {
            const response = await fetch(url.toString(), {
              headers: { Accept: 'text/event-stream' },
              signal: controller.signal,
            });
            if (!response.ok) {
              throw new Error(`Service event endpoint returned HTTP ${response.status}`);
            }
            const reader = response.body?.getReader();
            if (!reader) {
              throw new Error('The environment does not expose a streaming response body.');
            }

            retryCount = 0;
            const decoder = new TextDecoder();
            let buffer = '';
            try {
              while (active) {
                const { done, value } = await reader.read();
                if (done) break;
                buffer += decoder.decode(value, { stream: true });
                // SSE frames may use LF or CRLF line endings.
                const chunks = buffer.split(/\r?\n\r?\n/);
                buffer = chunks.pop() ?? '';
                for (const chunk of chunks) {
                  const event = parseSseChunk(chunk, events);
                  if (!event) continue;
                  // A malformed event or a throwing guest callback must not
                  // tear down the stream for subsequent events.
                  try {
                    handler(event);
                  } catch (error) {
                    console.warn('[Actrium] Service event handler failed:', error);
                  }
                }
              }
            } finally {
              reader.releaseLock();
            }
          } catch (error) {
            if (!active) break;
            console.warn('[Actrium] Service event stream failed:', error);
          }

          if (!active) break;
          // Reconnect with exponential backoff, capped at 30 seconds.
          const delayMs = Math.min(1000 * 2 ** retryCount, 30_000);
          retryCount = Math.min(retryCount + 1, 5);
          await new Promise((resolve) => setTimeout(resolve, delayMs));
        }
      };

      void consume();

      return () => {
        active = false;
        controller.abort();
      };
    },
  };
}

/** Parse one `text/event-stream` chunk into a ServiceEvent, or return null. */
function parseSseChunk(chunk: string, allowedEvents: string[]): ServiceEvent | null {
  let eventName: string | undefined;
  const dataLines: string[] = [];

  for (const line of chunk.split('\n')) {
    if (line.startsWith('event:')) {
      eventName = line.slice(6).trim();
    } else if (line.startsWith('data:')) {
      dataLines.push(line.slice(5).trim());
    }
    // Ignore `id:`, `retry:` and comments for the v1 service event subset.
  }

  if (!eventName || !allowedEvents.includes(eventName) || dataLines.length === 0) {
    return null;
  }

  try {
    const data = JSON.parse(dataLines.join('\n')) as { payloadBase64?: unknown };
    if (typeof data.payloadBase64 !== 'string' || data.payloadBase64.length === 0) {
      return null;
    }
    return { event: eventName, payloadBase64: data.payloadBase64 };
  } catch {
    return null;
  }
}

/**
 * Extract the allowed service event names from a capability manifest.
 *
 * The manifest may name the module as `service` (v1 canonical) or
 * `host:service` (older form). Returns an empty array when the manifest has no
 * service event declarations.
 */
export function serviceEventsFromManifest(
  manifest: CapabilityManifestV1
): string[] {
  const entry = manifest.hostModules['service'] ?? manifest.hostModules['host:service'];
  return entry?.events ?? [];
}

