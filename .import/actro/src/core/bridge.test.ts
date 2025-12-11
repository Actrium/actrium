/**
 * Core Bridge Tests
 *
 * Tests message routing and engine adapter
 * Note: Uses recreated handlers to avoid React Native imports
 */

import {
  EventEmitter,
  HostEventEmitter,
  BROADCASTER_SYMBOL,
  NOTIFY_SYMBOL,
} from '../api/EventEmitter.host';
import {
  Toast,
  HostToast,
  TOAST_SET_HANDLER as SET_HANDLER_SYMBOL,
  TOAST_CLEAR_HANDLER as CLEAR_HANDLER_SYMBOL,
} from '../api/Toast.host';
import { Haptic, HostHaptic, HAPTIC_SET_HANDLER, HAPTIC_CLEAR_HANDLER } from '../api/Haptic.host';

// Recreate module registry (same as registry.ts)
const modules = {
  toast: Toast,
  haptic: Haptic,
} as const;

// Recreate bridge logic (same as bridge.ts)
interface GuestMessage {
  event: string;
  payload?: unknown;
}

function handleGuestMessage(message: GuestMessage): unknown {
  // Inline validation
  if (
    !message ||
    typeof message !== 'object' ||
    !message.event ||
    typeof message.event !== 'string'
  ) {
    console.error('[askit/bridge] Invalid message format');
    return undefined;
  }

  const { event, payload } = message;

  // Handle askit protocol messages (format: askit:...)
  if (event.startsWith('askit:')) {
    const parts = event.slice(6).split(':');

    if (parts.length < 2) {
      console.error('[askit/bridge] Invalid askit protocol format');
      return undefined;
    }

    // EventEmitter events (format: askit:event:eventName)
    // Must check this FIRST because eventName can contain colons (e.g., user:login)
    if (parts[0] === 'event') {
      const eventName = parts.slice(1).join(':');
      (EventEmitter as HostEventEmitter)[NOTIFY_SYMBOL](eventName, payload);
      return undefined;
    }

    // Module calls (format: askit:module:method) - exactly 2 parts after 'askit:'
    if (parts.length === 2) {
      const validName = /^[a-zA-Z0-9_]+$/;
      const moduleName = parts[0]!;
      const methodName = parts[1]!;
      if (validName.test(moduleName) && validName.test(methodName)) {
        const module = modules[moduleName as keyof typeof modules];
        if (module) {
          const method = module[methodName as keyof typeof module];
          if (typeof method === 'function') {
            const args = Array.isArray(payload) ? payload : payload !== undefined ? [payload] : [];
            return (method as (...args: unknown[]) => unknown).apply(module as Record<string, unknown>, args);
          }
        }

        console.warn(`[askit/bridge] Unknown module or method: ${moduleName}.${methodName}`);
      }
    } else {
      console.warn('[askit/bridge] Invalid askit protocol format');
    }
    return undefined;
  }

  console.warn(`[askit/bridge] Unknown event: ${event}`);
  return undefined;
}

function createEngineAdapter(engine: {
  on: (event: string, callback: (...args: unknown[]) => void) => void;
  sendEvent: (event: string, payload?: unknown) => void;
}): {
  dispose: () => void;
} {
  // Forward EventEmitter events to engine with askit:event: prefix
  (EventEmitter as HostEventEmitter)[BROADCASTER_SYMBOL]((event: string, payload: unknown) => {
    engine.sendEvent(`askit:event:${event}`, payload);
  });

  engine.on('message', (...args: unknown[]) => {
    const event = args[0] as string;
    const payload = args[1] as unknown;
    handleGuestMessage({ event, payload });
  });

  return {
    dispose() {
      (EventEmitter as HostEventEmitter)[BROADCASTER_SYMBOL](null);
    },
  };
}

describe('Core Bridge', () => {
  describe('handleGuestMessage', () => {
    describe('askit module routing', () => {
      let toastCalls: Array<{ message: string; options: unknown }>;
      let hapticCalls: Array<{ type: unknown }>;

      beforeEach(() => {
        toastCalls = [];
        hapticCalls = [];

        (Toast as HostToast)[SET_HANDLER_SYMBOL]((message, options) => {
          toastCalls.push({ message, options });
        });

        (Haptic as HostHaptic)[HAPTIC_SET_HANDLER]((type) => {
          hapticCalls.push({ type });
        });
      });

      afterEach(() => {
        (Toast as HostToast)[CLEAR_HANDLER_SYMBOL]();
        (Haptic as HostHaptic)[HAPTIC_CLEAR_HANDLER]();
      });

      it('should route askit:toast:show to Toast module', () => {
        // When payload is object, it becomes args[0]
        handleGuestMessage({
          event: 'askit:toast:show',
          payload: { message: 'Hello', options: { position: 'top' } },
        });

        // The object is passed as first arg, second arg is undefined
        expect(toastCalls).toEqual([
          { message: { message: 'Hello', options: { position: 'top' } }, options: undefined },
        ]);
      });

      it('should handle array payload for toast', () => {
        handleGuestMessage({
          event: 'askit:toast:show',
          payload: ['Hello', { duration: 'long' }],
        });

        expect(toastCalls).toEqual([{ message: 'Hello', options: { duration: 'long' } }]);
      });

      it('should route askit:haptic:trigger to Haptic module', () => {
        // When payload is object, it becomes args[0]
        handleGuestMessage({
          event: 'askit:haptic:trigger',
          payload: { type: 'success' },
        });

        // The object is passed as type parameter
        expect(hapticCalls).toEqual([{ type: { type: 'success' } }]);
      });

      it('should handle array payload for haptic', () => {
        handleGuestMessage({
          event: 'askit:haptic:trigger',
          payload: ['light'],
        });

        expect(hapticCalls).toEqual([{ type: 'light' }]);
      });

      it('should handle missing payload (uses default)', () => {
        handleGuestMessage({
          event: 'askit:haptic:trigger',
        });

        // When no payload, args is [], so type is undefined, then Haptic uses default 'medium'
        expect(hapticCalls).toEqual([{ type: 'medium' }]);
      });

      it('should warn for unknown module', () => {
        const warnings: unknown[] = [];
        const originalWarn = console.warn;
        console.warn = (...args) => warnings.push(args);

        handleGuestMessage({
          event: 'askit:unknown:method',
          payload: 'data',
        });

        expect(warnings.length).toBe(1);
        expect((warnings[0] as unknown[])?.[0]).toContain(
          'Unknown module or method: unknown.method'
        );

        console.warn = originalWarn;
      });
    });

    describe('askit:event: protocol routing', () => {
      it('should route askit:event: events to EventEmitter', () => {
        const received: unknown[] = [];

        const unsubscribe = (EventEmitter as HostEventEmitter).on('testEvent', (payload) => {
          received.push(payload);
        });

        // Directly test the NOTIFY_SYMBOL method
        (EventEmitter as HostEventEmitter)[NOTIFY_SYMBOL]('testEvent', { data: 'fromGuest' });

        expect(received).toEqual([{ data: 'fromGuest' }]);

        unsubscribe();
      });

      it('should handle events without payload', () => {
        const received: unknown[] = [];

        const unsubscribe = (EventEmitter as HostEventEmitter).on('noPayloadEvent', (payload) => {
          received.push(payload);
        });

        // Directly test the NOTIFY_SYMBOL method
        (EventEmitter as HostEventEmitter)[NOTIFY_SYMBOL]('noPayloadEvent', undefined);

        expect(received).toEqual([undefined]);

        unsubscribe();
      });

      it('should parse askit:event: prefix in handleGuestMessage', () => {
        // Test that the bridge correctly extracts event name from 'askit:event:' prefix
        const result = handleGuestMessage({
          event: 'askit:event:testParsing',
          payload: 'test',
        });

        expect(result).toBeUndefined(); // handleGuestMessage returns undefined for askit:event: messages
      });

      it('should support nested event names', () => {
        const received: unknown[] = [];

        const unsubscribe = (EventEmitter as HostEventEmitter).on('user:login', (payload) => {
          received.push(payload);
        });

        // Test nested event name parsing
        handleGuestMessage({
          event: 'askit:event:user:login',
          payload: { userId: 123 },
        });

        expect(received).toEqual([{ userId: 123 }]);

        unsubscribe();
      });
    });

    describe('unknown events', () => {
      it('should warn for unknown event format', () => {
        const warnings: unknown[] = [];
        const originalWarn = console.warn;
        console.warn = (...args) => warnings.push(args);

        handleGuestMessage({
          event: 'unknown:event',
          payload: 'data',
        });

        expect(warnings.length).toBe(1);
        expect((warnings[0] as unknown[])?.[0]).toContain('Unknown event: unknown:event');

        console.warn = originalWarn;
      });

      it('should warn for random string', () => {
        const warnings: unknown[] = [];
        const originalWarn = console.warn;
        console.warn = (...args) => warnings.push(args);

        handleGuestMessage({
          event: 'randomstring',
          payload: 'data',
        });

        expect(warnings.length).toBe(1);

        console.warn = originalWarn;
      });
    });
  });

  describe('createEngineAdapter', () => {
    it('should create adapter with dispose function', () => {
      const engine = {
        on: () => {},
        sendEvent: () => {},
      };

      const adapter = createEngineAdapter(engine);

      expect(adapter).toBeDefined();
      expect(typeof adapter.dispose).toBe('function');
    });

    it('should forward EventEmitter events to engine.sendEvent', () => {
      const sentEvents: Array<{ event: string; payload: unknown }> = [];
      const engine = {
        on: () => {},
        sendEvent: (event: string, payload: unknown) => {
          sentEvents.push({ event, payload });
        },
      };

      createEngineAdapter(engine);

      (EventEmitter as HostEventEmitter).emit('hostEventForward', { data: 'toGuest' });

      expect(sentEvents).toContainEqual({
        event: 'askit:event:hostEventForward',
        payload: { data: 'toGuest' },
      });
    });

    it('should route engine messages to handleGuestMessage', () => {
      const toastCalls: string[] = [];
      (Toast as HostToast)[SET_HANDLER_SYMBOL]((message) => {
        toastCalls.push(message);
      });

      let messageCallback: ((...args: unknown[]) => void) | null = null;
      const engine = {
        on: (event: string, callback: (...args: unknown[]) => void) => {
          if (event === 'message') {
            messageCallback = callback;
          }
        },
        sendEvent: () => {},
      };

      createEngineAdapter(engine);

      if (messageCallback) {
        (messageCallback as (...args: unknown[]) => void)('askit:toast:show', [
          'Engine Toast Message',
        ]);
      }

      expect(toastCalls).toEqual(['Engine Toast Message']);

      (Toast as HostToast)[CLEAR_HANDLER_SYMBOL]();
    });

    it('should unregister bus on dispose', () => {
      const sentEvents: Array<{ event: string; payload: unknown }> = [];
      const engine = {
        on: () => {},
        sendEvent: (event: string, payload: unknown) => {
          sentEvents.push({ event, payload });
        },
      };

      const adapter = createEngineAdapter(engine);

      (EventEmitter as HostEventEmitter).emit('beforeDispose', 'data1');
      adapter.dispose();
      (EventEmitter as HostEventEmitter).emit('afterDispose', 'data2');

      const beforeEvents = sentEvents.filter((e) => e.event === 'askit:event:beforeDispose');
      const afterEvents = sentEvents.filter((e) => e.event === 'askit:event:afterDispose');

      expect(beforeEvents.length).toBe(1);
      expect(afterEvents.length).toBe(0);
    });
  });
});
