/**
 * EventEmitter Guest - Additional Coverage Tests
 * Tests for removeAllListeners and other uncovered methods
 */

import { GuestEventEmitter } from './EventEmitter.guest';

describe('EventEmitter (Guest) - Additional Coverage', () => {
  type SandboxGlobal = typeof globalThis & {
    __sendEventToHost?: (eventName: string, payload?: unknown) => void;
    __useHostEvent?: (eventName: string, callback: (payload: unknown) => void) => () => void;
  };

  const sandboxGlobal = globalThis as SandboxGlobal;

  let emitter: GuestEventEmitter;
  let originalSendToHost: unknown;
  let originalUseHostEvent: unknown;

  beforeEach(() => {
    originalSendToHost = sandboxGlobal.__sendEventToHost;
    originalUseHostEvent = sandboxGlobal.__useHostEvent;

    sandboxGlobal.__sendEventToHost = () => {};
    sandboxGlobal.__useHostEvent = () => () => {};

    emitter = new GuestEventEmitter();
  });

  afterEach(() => {
    // Clear any pending timers
    if (emitter && '_clearRetryTimer' in emitter) {
      (emitter as unknown as { _clearRetryTimer: () => void })._clearRetryTimer();
    }

    sandboxGlobal.__sendEventToHost = originalSendToHost as SandboxGlobal['__sendEventToHost'];
    sandboxGlobal.__useHostEvent = originalUseHostEvent as SandboxGlobal['__useHostEvent'];
  });

  describe('removeAllListeners', () => {
    it('should remove all listeners when called without arguments', () => {
      const calls: unknown[] = [];

      emitter.on('event1', (data) => calls.push(data));
      emitter.on('event2', (data) => calls.push(data));
      emitter.on('user:*', (data) => calls.push(data));

      // Remove all
      emitter.removeAllListeners();

      // Simulate events - should not trigger any callbacks
      emitter._simulateHostEvent('event1', 'test1');
      emitter._simulateHostEvent('event2', 'test2');
      emitter._simulateHostEvent('user:login', 'test3');

      expect(calls.length).toBe(0);
    });

    it('should remove only exact match listeners for specific event', () => {
      const calls: unknown[] = [];

      emitter.on('remove:me', (data) => calls.push(['removed', data]));
      emitter.on('keep:me', (data) => calls.push(['kept', data]));

      // Remove specific event
      emitter.removeAllListeners('remove:me');

      emitter._simulateHostEvent('remove:me', 'data1');
      emitter._simulateHostEvent('keep:me', 'data2');

      expect(calls).toEqual([['kept', 'data2']]);
    });

    it('should remove pattern listeners for specific pattern', () => {
      const calls: unknown[] = [];

      emitter.on('user:*', (data) => calls.push(['pattern', data]));
      emitter.on('api:*', (data) => calls.push(['api', data]));

      // Remove pattern listeners
      emitter.removeAllListeners('user:*');

      emitter._simulateHostEvent('user:login', 'data1');
      emitter._simulateHostEvent('api:call', 'data2');

      expect(calls).toEqual([['api', 'data2']]);
    });
  });
});
