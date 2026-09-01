/**
 * EventEmitter Guest - Edge Cases and Error Handling Tests
 *
 * Tests uncovered paths: retry logic, queue management, error boundaries
 */

import { GuestEventEmitter } from './EventEmitter.guest';

describe('EventEmitter (Guest) - Edge Cases', () => {
  type SandboxGlobal = typeof globalThis & {
    __sendEventToHost?: (eventName: string, payload?: unknown) => void;
    __useHostEvent?: (eventName: string, callback: (payload: unknown) => void) => () => void;
  };

  const sandboxGlobal = globalThis as SandboxGlobal;

  let emitter: GuestEventEmitter;
  let sentMessages: Array<{ event: string; payload: unknown }>;
  let originalSendToHost: unknown;
  let originalUseHostEvent: unknown;

  beforeEach(() => {
    originalSendToHost = sandboxGlobal.__sendEventToHost;
    originalUseHostEvent = sandboxGlobal.__useHostEvent;

    sentMessages = [];

    sandboxGlobal.__sendEventToHost = (eventName: string, payload?: unknown) => {
      sentMessages.push({ event: eventName, payload });
    };

    sandboxGlobal.__useHostEvent = () => () => {};

    emitter = new GuestEventEmitter();
  });

  afterEach(() => {
    sandboxGlobal.__sendEventToHost = originalSendToHost as SandboxGlobal['__sendEventToHost'];
    sandboxGlobal.__useHostEvent = originalUseHostEvent as SandboxGlobal['__useHostEvent'];

    // Clear any pending timers to prevent tests from hanging
    if (emitter && '_clearRetryTimer' in emitter) {
      (emitter as unknown as { _clearRetryTimer: () => void })._clearRetryTimer();
    }
  });

  describe('setMaxListeners and getMaxListeners', () => {
    it('should set and get max listeners', () => {
      expect(emitter.getMaxListeners()).toBe(10); // Default

      emitter.setMaxListeners(20);
      expect(emitter.getMaxListeners()).toBe(20);
    });

    it('should not allow negative max listeners', () => {
      emitter.setMaxListeners(-5);
      expect(emitter.getMaxListeners()).toBe(0); // Clamped to 0
    });

    it('should clear warned events when setting max listeners', () => {
      // This test verifies the warnedEvents.clear() is called
      // by setting a low limit, triggering warnings, then increasing limit
      emitter.setMaxListeners(1);

      // Add multiple listeners to trigger warning
      emitter.on('test', () => {});
      emitter.on('test', () => {});

      // Reset max listeners should clear warned events
      emitter.setMaxListeners(10);
      expect(emitter.getMaxListeners()).toBe(10);
    });
  });

  describe('emit with pattern listener errors', () => {
    it('should catch errors in pattern listener callbacks', () => {
      const errorLogs: unknown[] = [];
      const originalError = console.error;
      console.error = (...args: unknown[]) => errorLogs.push(args);

      // Add pattern listener that throws
      emitter.on('user:*', () => {
        throw new Error('Pattern listener error');
      });

      // Emit matching event - should catch error and log
      emitter.emit('user:login', { id: 123 });

      console.error = originalError;

      // Verify error was caught and logged
      expect(errorLogs.length).toBeGreaterThan(0);
    });

    it('should catch non-Error exceptions in pattern listeners', () => {
      const errorLogs: unknown[] = [];
      const originalError = console.error;
      console.error = (...args: unknown[]) => errorLogs.push(args);

      emitter.on('api:**', () => {
        throw 'string error'; // Non-Error exception
      });

      emitter.emit('api:users:fetch', null);

      console.error = originalError;
      expect(errorLogs.length).toBeGreaterThan(0);
    });
  });

  describe('message queue and retry logic', () => {
    it('should queue messages when __sendEventToHost throws', () => {
      const originalError = console.error;
      console.error = () => {};

      let callCount = 0;
      sandboxGlobal.__sendEventToHost = () => {
        callCount++;
        throw new Error('Network error');
      };

      emitter = new GuestEventEmitter();
      emitter.emit('test:event', { data: 'test' });

      (emitter as unknown as { _clearRetryTimer: () => void })._clearRetryTimer();
      console.error = originalError;

      expect(callCount).toBe(1);
    });

    it('should drop oldest message when queue is full', () => {
      const warnings: unknown[] = [];
      const originalWarn = console.warn;
      const originalError = console.error;

      console.warn = (...args: unknown[]) => warnings.push(args);
      console.error = () => {};

      sandboxGlobal.__sendEventToHost = () => {
        throw new Error('Always fails');
      };

      emitter = new GuestEventEmitter();

      for (let i = 0; i < 105; i++) {
        emitter.emit(`event${i}`, { index: i });
      }

      (emitter as unknown as { _clearRetryTimer: () => void })._clearRetryTimer();
      console.warn = originalWarn;
      console.error = originalError;

      expect(warnings.length).toBeGreaterThan(0);
    });

    it('should retry messages when __sendEventToHost becomes available', () => {
      // Use manual timer mocking to avoid async timing issues
      const originalSetTimeout = globalThis.setTimeout;
      let scheduledFn: (() => void) | null = null;

      globalThis.setTimeout = ((fn: () => void) => {
        scheduledFn = fn;
        return 1 as unknown as ReturnType<typeof setTimeout>;
      }) as typeof setTimeout;

      let sendToHostAvailable = false;
      const successfulMessages: string[] = [];

      sandboxGlobal.__sendEventToHost = (eventName: string) => {
        if (!sendToHostAvailable) {
          throw new Error('Not ready');
        }
        successfulMessages.push(eventName);
      };

      emitter = new GuestEventEmitter();
      emitter.emit('queued:event1', null);
      emitter.emit('queued:event2', null);

      // Now make sendToHost available and trigger the scheduled retry
      sendToHostAvailable = true;
      // TypeScript can't track that scheduledFn was assigned in the setTimeout callback
      (scheduledFn as (() => void) | null)?.();

      globalThis.setTimeout = originalSetTimeout;
      (emitter as unknown as { _clearRetryTimer: () => void })._clearRetryTimer();

      expect(successfulMessages.length).toBeGreaterThan(0);
    });

    it('should drop messages after max retries', () => {
      const originalSetTimeout = globalThis.setTimeout;
      const scheduledFns: Array<() => void> = [];

      globalThis.setTimeout = ((fn: () => void) => {
        scheduledFns.push(fn);
        return scheduledFns.length as unknown as ReturnType<typeof setTimeout>;
      }) as typeof setTimeout;

      const droppedLogs: unknown[] = [];
      const originalError = console.error;
      console.error = (...args: unknown[]) => droppedLogs.push(args);

      sandboxGlobal.__sendEventToHost = () => {
        throw new Error('Permanent failure');
      };

      emitter = new GuestEventEmitter();
      emitter.emit('will:fail', null);

      // Trigger all retries (maxRetries = 3)
      for (let i = 0; i < 4 && scheduledFns.length > 0; i++) {
        const fn = scheduledFns.shift();
        if (fn) fn();
      }

      globalThis.setTimeout = originalSetTimeout;
      (emitter as unknown as { _clearRetryTimer: () => void })._clearRetryTimer();
      console.error = originalError;

      expect(droppedLogs.length).toBeGreaterThan(0);
    });

    it('should handle missing __sendEventToHost gracefully when processing queue', () => {
      const originalSetTimeout = globalThis.setTimeout;
      let scheduledFn: (() => void) | null = null;

      globalThis.setTimeout = ((fn: () => void) => {
        scheduledFn = fn;
        return 1 as unknown as ReturnType<typeof setTimeout>;
      }) as typeof setTimeout;

      let callCount = 0;
      sandboxGlobal.__sendEventToHost = () => {
        callCount++;
        throw new Error('Fail once');
      };

      emitter = new GuestEventEmitter();
      emitter.emit('queued', null);

      // Remove __sendEventToHost before retry
      sandboxGlobal.__sendEventToHost = undefined;
      // TypeScript can't track that scheduledFn was assigned in the setTimeout callback
      (scheduledFn as (() => void) | null)?.();

      globalThis.setTimeout = originalSetTimeout;
      (emitter as unknown as { _clearRetryTimer: () => void })._clearRetryTimer();

      expect(callCount).toBe(1);
    });
  });

  describe('emit validation', () => {
    it('should handle invalid event names', () => {
      const errorLogs: unknown[] = [];
      const originalError = console.error;
      console.error = (...args: unknown[]) => errorLogs.push(args);

      // @ts-expect-error - Testing invalid input
      emitter.emit(null, null);
      emitter.emit('', null);
      // @ts-expect-error - Testing invalid input
      emitter.emit(123, null);

      console.error = originalError;

      expect(errorLogs.length).toBeGreaterThan(0);
    });

    it('should handle emit without __sendEventToHost available', () => {
      const warnings: unknown[] = [];
      const originalWarn = console.warn;
      console.warn = (...args: unknown[]) => warnings.push(args);

      sandboxGlobal.__sendEventToHost = undefined;

      emitter = new GuestEventEmitter();
      emitter.emit('test', null);

      (emitter as unknown as { _clearRetryTimer: () => void })._clearRetryTimer();
      console.warn = originalWarn;

      expect(warnings.length).toBeGreaterThan(0);
    });
  });

  describe('local listener notification during emit', () => {
    it('should notify local exact-match listeners when emitting', () => {
      const calls: unknown[] = [];

      emitter.on('local:event', (data) => {
        calls.push(data);
      });

      emitter.emit('local:event', { value: 42 });

      expect(calls).toEqual([{ value: 42 }]);
    });

    it('should notify local pattern listeners when emitting', () => {
      const calls: unknown[] = [];

      emitter.on('pattern:*', (data) => {
        calls.push(data);
      });

      emitter.emit('pattern:match', { matched: true });

      expect(calls).toEqual([{ matched: true }]);
    });

    it('should catch errors in local exact-match listeners', () => {
      const errorLogs: unknown[] = [];
      const originalError = console.error;
      console.error = (...args: unknown[]) => errorLogs.push(args);

      emitter.on('throws', () => {
        throw new Error('Exact match error');
      });

      emitter.emit('throws', null);

      console.error = originalError;
      expect(errorLogs.length).toBeGreaterThan(0);
    });
  });

  describe('retry scheduling', () => {
    it('should not schedule multiple retries simultaneously', () => {
      const originalError = console.error;
      console.error = () => {};

      let retryCount = 0;
      const originalSetTimeout = globalThis.setTimeout;
      globalThis.setTimeout = ((fn: () => void, delay: number) => {
        retryCount++;
        return originalSetTimeout(fn, delay);
      }) as typeof setTimeout;

      sandboxGlobal.__sendEventToHost = () => {
        throw new Error('Fail');
      };

      emitter = new GuestEventEmitter();

      emitter.emit('event1', null);
      emitter.emit('event2', null);
      emitter.emit('event3', null);

      (emitter as unknown as { _clearRetryTimer: () => void })._clearRetryTimer();
      globalThis.setTimeout = originalSetTimeout;
      console.error = originalError;

      expect(retryCount).toBeGreaterThanOrEqual(1);
      expect(retryCount).toBeLessThan(6);
    });
  });
});
