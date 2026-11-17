import type { CapabilityManifestV1 } from './capability-verify';
import type { ServiceEvent } from './contract';

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
 * The source opens an `EventSource` against `eventsUrl` and listens for events
 * whose `type` matches one of the allowed event names. The event `data` must be
 * a JSON object with `{ payloadBase64: string }`. This is a v1 transport option
 * for Service deployments that expose an SSE endpoint; the native WebRTC path
 * is preferred for attached Realm Services.
 */
export function createHttpServiceEventSource(
  eventsUrl: string,
  events: string[]
): ServiceEventSource {
  return {
    subscribe(handler) {
      if (typeof EventSource === 'undefined') {
        throw new Error(
          'Server-Sent Events are not available in this environment.'
        );
      }

      const url = new URL(eventsUrl, typeof location !== 'undefined' ? location.href : undefined);
      for (const event of events) {
        url.searchParams.append('event', event);
      }

      const eventSource = new EventSource(url.toString());
      const listeners = new Map<string, (messageEvent: MessageEvent) => void>();

      for (const event of events) {
        const listener = (messageEvent: MessageEvent) => {
          let data: unknown;
          try {
            data = JSON.parse(String(messageEvent.data));
          } catch {
            return;
          }
          const payloadBase64 =
            data && typeof data === 'object' && 'payloadBase64' in data
              ? String((data as { payloadBase64: unknown }).payloadBase64)
              : '';
          handler({ event, payloadBase64 });
        };
        listeners.set(event, listener);
        eventSource.addEventListener(event, listener);
      }

      return () => {
        for (const [event, listener] of listeners) {
          eventSource.removeEventListener(event, listener);
        }
        eventSource.close();
      };
    },
  };
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

