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

export type NativeServiceEventModule = {
  /**
   * Start listening for service events on behalf of a tab.
   *
   * The native runtime calls `listener` with `{ event: string, payloadBase64: string }`
   * whenever the attached Service sends an allowed event.
   */
  addServiceEventListener?: (
    tabId: string,
    events: string[],
    listener: (event: ServiceEvent) => void
  ) => () => void;
};

/**
 * Create an event source backed by the native RealmService module.
 *
 * The native module is expected to manage the WebRTC/data-channel subscription
 * to the attached Service actor and surface events through
 * `addServiceEventListener(tabId, events, listener)`. If the module does not
 * expose that method, the source fails closed at subscribe time.
 */
export function createNativeServiceEventSource(
  nativeModule: NativeServiceEventModule | undefined,
  tabId: string,
  events: string[]
): ServiceEventSource {
  return {
    subscribe(handler) {
      if (!nativeModule?.addServiceEventListener) {
        throw new Error(
          'The native Realm Service event bridge is unavailable.'
        );
      }
      return nativeModule.addServiceEventListener(tabId, events, handler);
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

      const consume = async () => {
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

          const decoder = new TextDecoder();
          let buffer = '';
          try {
            while (active) {
              const { done, value } = await reader.read();
              if (done) break;
              buffer += decoder.decode(value, { stream: true });
              const chunks = buffer.split('\n\n');
              buffer = chunks.pop() ?? '';
              for (const chunk of chunks) {
                const event = parseSseChunk(chunk, events);
                if (event) handler(event);
              }
            }
          } finally {
            reader.releaseLock();
          }
        } catch (error) {
          if (active) {
            console.warn('[Actrium] Service event stream failed:', error);
          }
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

