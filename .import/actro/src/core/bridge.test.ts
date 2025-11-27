/**
 * Core Bridge Tests
 *
 * Tests message routing and engine adapter
 * Note: Uses recreated handlers to avoid React Native imports
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { Bus, NativeBus } from '../api/Bus.native';
import { Toast as ToastModule, NativeToast } from '../api/Toast.native';
import { Haptic as HapticModule, NativeHaptic } from '../api/Haptic.native';

// Recreate module handlers (same logic as registry.ts)
interface ModuleHandler {
  handle: (method: string, args: unknown[]) => unknown;
}

const ToastHandler: ModuleHandler = {
  handle(method: string, args: unknown[]) {
    if (method === 'show') {
      const [message, options] = args as [string, unknown];
      ToastModule.show(message, options as Parameters<typeof ToastModule.show>[1]);
    }
  },
};

const HapticHandler: ModuleHandler = {
  handle(method: string, args: unknown[]) {
    if (method === 'trigger') {
      const [type] = args as [Parameters<typeof HapticModule.trigger>[0]];
      HapticModule.trigger(type);
    }
  },
};

const AskitModules: Record<string, ModuleHandler> = {
  toast: ToastHandler,
  haptic: HapticHandler,
};

// Recreate bridge logic (same as bridge.ts)
interface GuestMessage {
  event: string;
  payload?: unknown;
}

function parseAskitEvent(event: string): { module: string; method: string } | null {
  if (!event.startsWith('askit:')) {
    return null;
  }

  const parts = event.slice(6).split(':');
  if (parts.length !== 2) {
    return null;
  }

  const module = parts[0];
  const method = parts[1];
  if (!module || !method) {
    return null;
  }

  return {
    module,
    method,
  };
}

function handleGuestMessage(message: GuestMessage): unknown {
  const { event, payload } = message;

  const parsed = parseAskitEvent(event);
  if (parsed) {
    const handler = AskitModules[parsed.module];
    if (handler) {
      const args = Array.isArray(payload) ? payload : payload !== undefined ? [payload] : [];
      return handler.handle(parsed.method, args);
    }
    console.warn(`[askit/bridge] Unknown module: ${parsed.module}`);
    return undefined;
  }

  if (event.startsWith('bus:')) {
    const busEvent = event.slice(4);
    (Bus as NativeBus)._handleEngineMessage(busEvent, payload);
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
  const unregisterBus = (Bus as NativeBus)._registerEngine((event: string, payload: unknown) => {
    engine.sendEvent(event, payload);
  });

  engine.on('message', (...args: unknown[]) => {
    const event = args[0] as string;
    const payload = args[1] as unknown;
    handleGuestMessage({ event, payload });
  });

  return {
    dispose() {
      unregisterBus();
    },
  };
}

describe('Core Bridge', () => {
  describe('parseAskitEvent', () => {
    it('should parse valid askit event', () => {
      expect(parseAskitEvent('askit:toast:show')).toEqual({
        module: 'toast',
        method: 'show',
      });
    });

    it('should return null for non-askit event', () => {
      expect(parseAskitEvent('bus:event')).toBeNull();
      expect(parseAskitEvent('other:event')).toBeNull();
    });

    it('should return null for invalid format', () => {
      expect(parseAskitEvent('askit:invalid')).toBeNull();
      expect(parseAskitEvent('askit:')).toBeNull();
      expect(parseAskitEvent('askit:a:b:c')).toBeNull();
    });
  });

  describe('handleGuestMessage', () => {
    describe('askit module routing', () => {
      let toastCalls: Array<{ message: string; options: unknown }>;
      let hapticCalls: Array<{ type: unknown }>;

      beforeEach(() => {
        toastCalls = [];
        hapticCalls = [];

        (ToastModule as NativeToast)._setCustomHandler((message, options) => {
          toastCalls.push({ message, options });
        });

        (HapticModule as NativeHaptic)._setCustomHandler((type) => {
          hapticCalls.push({ type });
        });
      });

      afterEach(() => {
        (ToastModule as NativeToast)._clearCustomHandler();
        (HapticModule as NativeHaptic)._clearCustomHandler();
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
        expect((warnings[0] as unknown[])?.[0]).toContain('Unknown module: unknown');

        console.warn = originalWarn;
      });
    });

    describe('bus event routing', () => {
      beforeEach(() => {
        // Create fresh bus instance for each test
        new NativeBus();
      });

      it('should route bus: events to Bus', () => {
        const received: unknown[] = [];

        (Bus as NativeBus).on('testEvent', (payload) => {
          received.push(payload);
        });

        handleGuestMessage({
          event: 'bus:testEvent',
          payload: { data: 'fromGuest' },
        });

        expect(received).toEqual([{ data: 'fromGuest' }]);
      });

      it('should handle bus events without payload', () => {
        const received: unknown[] = [];

        (Bus as NativeBus).on('noPayloadBus', (payload) => {
          received.push(payload);
        });

        handleGuestMessage({
          event: 'bus:noPayloadBus',
        });

        expect(received).toEqual([undefined]);
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

    it('should forward Bus events to engine.sendEvent', () => {
      const sentEvents: Array<{ event: string; payload: unknown }> = [];
      const engine = {
        on: () => {},
        sendEvent: (event: string, payload: unknown) => {
          sentEvents.push({ event, payload });
        },
      };

      createEngineAdapter(engine);

      (Bus as NativeBus).emit('hostEventForward', { data: 'toGuest' });

      expect(sentEvents).toContainEqual({
        event: 'hostEventForward',
        payload: { data: 'toGuest' },
      });
    });

    it('should route engine messages to handleGuestMessage', () => {
      const toastCalls: string[] = [];
      (ToastModule as NativeToast)._setCustomHandler((message) => {
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

      (ToastModule as NativeToast)._clearCustomHandler();
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

      (Bus as NativeBus).emit('beforeDispose', 'data1');
      adapter.dispose();
      (Bus as NativeBus).emit('afterDispose', 'data2');

      const beforeEvents = sentEvents.filter((e) => e.event === 'beforeDispose');
      const afterEvents = sentEvents.filter((e) => e.event === 'afterDispose');

      expect(beforeEvents.length).toBe(1);
      expect(afterEvents.length).toBe(0);
    });
  });
});
