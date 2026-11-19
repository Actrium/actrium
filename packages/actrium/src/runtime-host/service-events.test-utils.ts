import type { ServiceEvent } from './contract';
import type {
  ServiceEventHandler,
  ServiceEventSource,
} from './service-events';

/** Create an event source that allows tests to inject events manually. */
export function createTestServiceEventSource(events: string[]): {
  source: ServiceEventSource;
  emit(event: ServiceEvent): void;
  subscriberCount(): number;
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
        throw new Error(`Event "${event.event}" is not in the allowed test event set.`);
      }
      for (const handler of [...handlers]) {
        try {
          handler(event);
        } catch (error) {
          console.warn('[Actrium] Service event handler failed:', error);
        }
      }
    },
    subscriberCount() {
      return handlers.size;
    },
  };
}
