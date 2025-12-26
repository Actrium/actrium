/**
 * Bus Guest Implementation Tests
 *
 * 100% real tests - simulating sandbox environment with real globals
 */

import { GuestEventEmitter } from './EventEmitter.guest';

describe('EventEmitter (Guest)', () => {
  type SandboxGlobal = typeof globalThis & {
    __sendEventToHost?: (eventName: string, payload?: unknown) => void;
    __useHostEvent?: (eventName: string, callback: (payload: unknown) => void) => () => void;
  };

  const sandboxGlobal = globalThis as SandboxGlobal;

  let emitter: GuestEventEmitter;
  let sentMessages: Array<{ event: string; payload: unknown }>;
  let originalSendToHost: unknown;
  let originalUseHostEvent: unknown;

  // Map of event name -> Set of callbacks (simulating rill's per-event subscription)
  let hostEventCallbacks: Map<string, Set<(payload: unknown) => void>>;

  beforeEach(() => {
    // Save original globals
    originalSendToHost = sandboxGlobal.__sendEventToHost;
    originalUseHostEvent = sandboxGlobal.__useHostEvent;

    // Reset test state
    sentMessages = [];
    hostEventCallbacks = new Map();

    // Setup simulated sandbox globals
    sandboxGlobal.__sendEventToHost = (eventName: string, payload?: unknown) => {
      sentMessages.push({ event: eventName, payload });
    };

    // Setup __useHostEvent to simulate rill's per-event subscription model
    sandboxGlobal.__useHostEvent = (eventName: string, callback: (payload: unknown) => void) => {
      if (!hostEventCallbacks.has(eventName)) {
        hostEventCallbacks.set(eventName, new Set());
      }
      hostEventCallbacks.get(eventName)!.add(callback);
      // Return unsubscribe function
      return () => {
        hostEventCallbacks.get(eventName)?.delete(callback);
      };
    };

    // Create fresh instance
    emitter = new GuestEventEmitter();
  });

  afterEach(() => {
    // Clear any pending timers to prevent tests from hanging
    if (emitter && '_clearRetryTimer' in emitter) {
      (emitter as unknown as { _clearRetryTimer: () => void })._clearRetryTimer();
    }

    // Restore original globals
    sandboxGlobal.__sendEventToHost = originalSendToHost as SandboxGlobal['__sendEventToHost'];
    sandboxGlobal.__useHostEvent = originalUseHostEvent as SandboxGlobal['__useHostEvent'];
  });

  describe('emit', () => {
    it('should send events to host through sendToHost', () => {
      emitter.emit('myEvent', { data: 'test' });

      expect(sentMessages).toEqual([{ event: 'myEvent', payload: { data: 'test' } }]);
    });

    it('should send event name as-is', () => {
      emitter.emit('customEvent', 'payload');

      expect(sentMessages[0]?.event).toBe('customEvent');
    });

    it('should handle emit without payload', () => {
      emitter.emit('noPayload');

      expect(sentMessages).toEqual([{ event: 'noPayload', payload: undefined }]);
    });

    it('should handle complex payloads', () => {
      const complexPayload = {
        array: [1, 2, 3],
        nested: { deep: { value: true } },
      };

      emitter.emit('complex', complexPayload);

      expect(sentMessages[0]?.payload).toEqual(complexPayload);
    });

    it('should also notify local listeners', () => {
      const received: unknown[] = [];

      emitter.on('localNotify', (payload) => {
        received.push(payload);
      });

      emitter.emit('localNotify', 'data');

      expect(received).toEqual(['data']);
      // And also sent to host
      expect(sentMessages.length).toBe(1);
    });

    it('should reject invalid event names', () => {
      const errors: unknown[] = [];
      const originalError = console.error;
      console.error = (...args) => errors.push(args);

      // @ts-expect-error - Testing invalid input
      emitter.emit(null);
      // @ts-expect-error - Testing invalid input
      emitter.emit(123);
      emitter.emit('');

      expect(sentMessages.length).toBe(0);
      expect(errors.length).toBe(3);
      expect(JSON.stringify(errors)).toContain('Invalid event name');

      console.error = originalError;
    });

    it('should handle __sendEventToHost throwing exception', () => {
      const errors: unknown[] = [];
      const originalError = console.error;
      console.error = (...args) => errors.push(args);

      // Mock __sendEventToHost to throw
      sandboxGlobal.__sendEventToHost = () => {
        throw new Error('Network failure');
      };

      // Create new bus instance
      const newBus = new GuestEventEmitter();
      newBus.emit('test', 'data');

      expect(errors.length).toBeGreaterThan(0);
      expect(JSON.stringify(errors)).toContain('Failed to send event');

      // Cleanup
      (newBus as unknown as { _clearRetryTimer: () => void })._clearRetryTimer();
      console.error = originalError;
    });
  });

  describe('on/off', () => {
    it('should receive events via on', () => {
      const received: unknown[] = [];

      emitter.on('test', (payload) => {
        received.push(payload);
      });

      emitter._simulateHostEvent('test', { from: 'host' });

      expect(received).toEqual([{ from: 'host' }]);
    });

    it('should return cleanup function from on()', () => {
      const received: unknown[] = [];
      const callback = (payload: unknown) => received.push(payload);

      const unsubscribe = emitter.on('cleanup', callback);

      emitter._simulateHostEvent('cleanup', 'before');
      expect(received).toEqual(['before']);

      unsubscribe();

      emitter._simulateHostEvent('cleanup', 'after');
      expect(received).toEqual(['before']); // 'after' should not be received
    });

    it('should receive events through __useHostEvent callback', () => {
      const received: unknown[] = [];

      emitter.on('hostCallback', (payload) => {
        received.push(payload);
      });

      // Simulate host calling back through per-event subscription
      const callbacks = hostEventCallbacks.get('hostCallback');
      if (callbacks) {
        callbacks.forEach((cb) => cb({ via: 'callback' }));
      }

      expect(received).toEqual([{ via: 'callback' }]);
    });

    it('should support multiple listeners for same event', () => {
      const received1: unknown[] = [];
      const received2: unknown[] = [];

      emitter.on('multi', (p) => received1.push(p));
      emitter.on('multi', (p) => received2.push(p));

      emitter._simulateHostEvent('multi', 'value');

      expect(received1).toEqual(['value']);
      expect(received2).toEqual(['value']);
    });

    it('should unsubscribe with off', () => {
      const received: unknown[] = [];
      const callback = (p: unknown) => received.push(p);

      emitter.on('offTest', callback);
      emitter._simulateHostEvent('offTest', 'first');

      emitter.off('offTest', callback);
      emitter._simulateHostEvent('offTest', 'second');

      expect(received).toEqual(['first']);
    });

    it('should only remove specified callback', () => {
      const received1: unknown[] = [];
      const received2: unknown[] = [];
      const callback1 = (p: unknown) => received1.push(p);
      const callback2 = (p: unknown) => received2.push(p);

      emitter.on('partial', callback1);
      emitter.on('partial', callback2);

      emitter.off('partial', callback1);
      emitter._simulateHostEvent('partial', 'value');

      expect(received1).toEqual([]);
      expect(received2).toEqual(['value']);
    });

    it('should handle off for non-existent event', () => {
      expect(() => emitter.off('nonExistent', () => {})).not.toThrow();
    });

    it('should clean up empty listener sets', () => {
      const callback = () => {};
      emitter.on('cleanup', callback);
      emitter.off('cleanup', callback);

      // Should not throw
      expect(() => emitter._simulateHostEvent('cleanup', 'data')).not.toThrow();
    });
  });

  describe('once', () => {
    it('should only fire once', () => {
      const received: unknown[] = [];

      emitter.once('onceTest', (payload) => {
        received.push(payload);
      });

      emitter._simulateHostEvent('onceTest', 'first');
      emitter._simulateHostEvent('onceTest', 'second');
      emitter._simulateHostEvent('onceTest', 'third');

      expect(received).toEqual(['first']);
    });

    it('should work with multiple once listeners', () => {
      const received1: unknown[] = [];
      const received2: unknown[] = [];

      emitter.once('multiOnce', (p) => received1.push(p));
      emitter.once('multiOnce', (p) => received2.push(p));

      emitter._simulateHostEvent('multiOnce', 'value');
      emitter._simulateHostEvent('multiOnce', 'value2');

      expect(received1).toEqual(['value']);
      expect(received2).toEqual(['value']);
    });

    it('should work together with on listeners', () => {
      const onceReceived: unknown[] = [];
      const onReceived: unknown[] = [];

      emitter.once('mixed', (p) => onceReceived.push(p));
      emitter.on('mixed', (p) => onReceived.push(p));

      emitter._simulateHostEvent('mixed', '1');
      emitter._simulateHostEvent('mixed', '2');

      expect(onceReceived).toEqual(['1']);
      expect(onReceived).toEqual(['1', '2']);
    });
  });

  describe('error handling', () => {
    it('should catch errors in listeners without breaking others', () => {
      const received: unknown[] = [];
      const consoleError = console.error;
      const errors: unknown[] = [];
      console.error = (...args) => errors.push(args);

      emitter.on('errorEvent', () => {
        throw new Error('Listener error');
      });
      emitter.on('errorEvent', (p) => received.push(p));

      emitter._simulateHostEvent('errorEvent', 'data');

      expect(received).toEqual(['data']);
      expect(errors.length).toBeGreaterThanOrEqual(1);

      console.error = consoleError;
    });

    it('should catch errors in emit local listeners', () => {
      const received: unknown[] = [];
      const consoleError = console.error;
      const errors: unknown[] = [];
      console.error = (...args) => errors.push(args);

      emitter.on('emitError', () => {
        throw new Error('Emit listener error');
      });
      emitter.on('emitError', (p) => received.push(p));

      emitter.emit('emitError', 'data');

      expect(received).toEqual(['data']);
      expect(errors.length).toBeGreaterThanOrEqual(1);

      console.error = consoleError;
    });
  });

  describe('without __sendEventToHost', () => {
    it('should warn when __sendEventToHost is not available', () => {
      // Remove __sendEventToHost
      sandboxGlobal.__sendEventToHost = undefined;

      const busWithoutHost = new GuestEventEmitter();

      const consoleWarn = console.warn;
      const warnings: unknown[] = [];
      console.warn = (...args) => warnings.push(args);

      busWithoutHost.emit('test', 'data');

      expect(JSON.stringify(warnings)).toContain('__sendEventToHost not available');

      // Cleanup
      (busWithoutHost as unknown as { _clearRetryTimer: () => void })._clearRetryTimer();
      console.warn = consoleWarn;
    });
  });

  describe('retry queue mechanism', () => {
    it('should warn and queue when __sendEventToHost is unavailable', () => {
      // Start without __sendEventToHost
      sandboxGlobal.__sendEventToHost = undefined;

      const retryEmitter = new GuestEventEmitter();
      const warnings: string[] = [];

      // Capture warnings
      const originalWarn = console.warn;
      console.warn = (...args) => warnings.push(args.join(' '));

      // Emit while __sendEventToHost is unavailable - should warn
      retryEmitter.emit('queued:event', { data: 'test' });

      // Should have warned about __sendEventToHost not available
      expect(warnings.some((w) => w.includes('__sendEventToHost not available'))).toBe(true);

      // Cleanup
      (retryEmitter as unknown as { _clearRetryTimer: () => void })._clearRetryTimer();
      console.warn = originalWarn;
    });

    it('should log error and queue when __sendEventToHost throws', () => {
      // __sendEventToHost throws error
      sandboxGlobal.__sendEventToHost = () => {
        throw new Error('Send failed');
      };

      const retryEmitter = new GuestEventEmitter();
      const errors: string[] = [];

      // Capture errors
      const originalError = console.error;
      console.error = (...args) => errors.push(JSON.stringify(args));

      // Emit - will fail
      retryEmitter.emit('failed:event', { data: 'test' });

      // Should have logged error about send failure
      expect(errors.some((e) => e.includes('Failed to send event'))).toBe(true);

      // Cleanup
      (retryEmitter as unknown as { _clearRetryTimer: () => void })._clearRetryTimer();
      console.error = originalError;
    });

    it('should not throw when clearing timer on fresh instance', () => {
      const retryEmitter = new GuestEventEmitter();

      // Should not throw
      expect(() => {
        (retryEmitter as unknown as { _clearRetryTimer: () => void })._clearRetryTimer();
      }).not.toThrow();
    });
  });

  describe('wildcard pattern matching', () => {
    it('should match single-level wildcard (user:*)', () => {
      const received: unknown[] = [];

      emitter.on('user:*', (payload: unknown) => {
        received.push(payload);
      });

      emitter._simulateHostEvent('user:login', { id: 1 });
      emitter._simulateHostEvent('user:logout', { id: 2 });
      emitter._simulateHostEvent('user:register', { id: 3 });
      emitter._simulateHostEvent('admin:login', { id: 4 }); // Should NOT match

      expect(received).toEqual([{ id: 1 }, { id: 2 }, { id: 3 }]);
    });

    it('should match multi-level wildcard (analytics:**)', () => {
      const received: unknown[] = [];

      emitter.on('analytics:**', (payload: unknown) => {
        received.push(payload);
      });

      emitter._simulateHostEvent('analytics:click', 'button');
      emitter._simulateHostEvent('analytics:page:view', 'home');
      emitter._simulateHostEvent('analytics:user:profile:update', 'avatar');
      emitter._simulateHostEvent('metrics:cpu', 50); // Should NOT match

      expect(received).toEqual(['button', 'home', 'avatar']);
    });

    it('should match middle wildcard (api:*:success)', () => {
      const received: unknown[] = [];

      emitter.on('api:*:success', (payload: unknown) => {
        received.push(payload);
      });

      emitter._simulateHostEvent('api:users:success', 'user_data');
      emitter._simulateHostEvent('api:posts:success', 'post_data');
      emitter._simulateHostEvent('api:users:error', 'error'); // Should NOT match

      expect(received).toEqual(['user_data', 'post_data']);
    });

    it('should support cleanup for pattern listeners', () => {
      const received: unknown[] = [];

      const unsubscribe = emitter.on('test:*', (payload: unknown) => {
        received.push(payload);
      });

      emitter._simulateHostEvent('test:one', 1);
      expect(received).toEqual([1]);

      unsubscribe();

      emitter._simulateHostEvent('test:two', 2);
      expect(received).toEqual([1]); // Should not receive test:two
    });

    it('should work with once() for pattern listeners', () => {
      const received: unknown[] = [];

      emitter.once('event:*', (payload: unknown) => {
        received.push(payload);
      });

      emitter._simulateHostEvent('event:first', 1);
      emitter._simulateHostEvent('event:second', 2);

      expect(received).toEqual([1]); // Should only receive first match
    });

    it('should trigger both exact and pattern listeners', () => {
      const exactReceived: unknown[] = [];
      const patternReceived: unknown[] = [];

      emitter.on('user:login', (payload: unknown) => {
        exactReceived.push(payload);
      });

      emitter.on('user:*', (payload: unknown) => {
        patternReceived.push(payload);
      });

      emitter._simulateHostEvent('user:login', 'data');

      expect(exactReceived).toEqual(['data']);
      expect(patternReceived).toEqual(['data']);
    });
  });

  describe('retry mechanism', () => {
    it('should catch and queue messages when __sendEventToHost throws error', () => {
      const errors: string[] = [];

      sandboxGlobal.__sendEventToHost = () => {
        throw new Error('Network failure');
      };

      const originalError = console.error;
      console.error = (...args: unknown[]) => {
        if (typeof args[0] === 'string') {
          errors.push(args[0]);
        }
      };

      emitter.emit('retry:test', 'data');

      // Should catch the error and log it
      expect(errors.length).toBeGreaterThanOrEqual(1);

      console.error = originalError;
    });
  });
});
