/**
 * Bus Guest Implementation Tests
 *
 * 100% real tests - simulating sandbox environment with real globals
 */

import { GuestEventEmitter } from './EventEmitter.guest';

describe('EventEmitter (Guest)', () => {
  type SandboxGlobal = typeof globalThis & {
    sendToHost?: (event: string, payload?: unknown) => void;
    onHostEvent?: (callback: (event: string, payload: unknown) => void) => void;
  };

  const sandboxGlobal = globalThis as SandboxGlobal;

  let emitter: GuestEventEmitter;
  let sentMessages: Array<{ event: string; payload: unknown }>;
  let originalSendToHost: unknown;
  let originalOnHostEvent: unknown;

  let hostEventCallback: ((event: string, payload: unknown) => void) | null = null;

  beforeEach(() => {
    // Save original globals
    originalSendToHost = sandboxGlobal.sendToHost;
    originalOnHostEvent = sandboxGlobal.onHostEvent;

    // Reset test state
    sentMessages = [];
    hostEventCallback = null;

    // Setup simulated sandbox globals
    sandboxGlobal.sendToHost = (event: string, payload?: unknown) => {
      sentMessages.push({ event, payload });
    };

    // Setup onHostEvent to capture the callback
    sandboxGlobal.onHostEvent = (callback: (event: string, payload: unknown) => void) => {
      hostEventCallback = callback;
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
    sandboxGlobal.sendToHost = originalSendToHost as SandboxGlobal['sendToHost'];
    sandboxGlobal.onHostEvent = originalOnHostEvent as SandboxGlobal['onHostEvent'];
  });

  describe('emit', () => {
    it('should send events to host through sendToHost', () => {
      emitter.emit('myEvent', { data: 'test' });

      expect(sentMessages).toEqual([{ event: 'askit:event:myEvent', payload: { data: 'test' } }]);
    });

    it('should prefix events with askit:event:', () => {
      emitter.emit('customEvent', 'payload');

      expect(sentMessages[0]?.event).toBe('askit:event:customEvent');
    });

    it('should handle emit without payload', () => {
      emitter.emit('noPayload');

      expect(sentMessages).toEqual([{ event: 'askit:event:noPayload', payload: undefined }]);
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

    it('should handle sendToHost throwing exception', () => {
      const errors: unknown[] = [];
      const originalError = console.error;
      console.error = (...args) => errors.push(args);

      // Mock sendToHost to throw
      sandboxGlobal.sendToHost = () => {
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

    it('should receive events through onHostEvent callback', () => {
      const received: unknown[] = [];

      emitter.on('hostCallback', (payload) => {
        received.push(payload);
      });

      // Simulate host calling back through onHostEvent
      if (hostEventCallback) {
        hostEventCallback('hostCallback', { via: 'callback' });
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

  describe('without sendToHost', () => {
    it('should warn when sendToHost is not available', () => {
      // Remove sendToHost
      sandboxGlobal.sendToHost = undefined;

      const busWithoutHost = new GuestEventEmitter();

      const consoleWarn = console.warn;
      const warnings: unknown[] = [];
      console.warn = (...args) => warnings.push(args);

      busWithoutHost.emit('test', 'data');

      expect(JSON.stringify(warnings)).toContain('sendToHost not available');

      // Cleanup
      (busWithoutHost as unknown as { _clearRetryTimer: () => void })._clearRetryTimer();
      console.warn = consoleWarn;
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
    it('should catch and queue messages when sendToHost throws error', () => {
      const errors: string[] = [];

      sandboxGlobal.sendToHost = () => {
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
