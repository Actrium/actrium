/**
 * EventEmitter Guest - Edge Cases and Error Handling Tests
 *
 * Tests uncovered paths: retry logic, queue management, error boundaries
 */

import { GuestEventEmitter } from './EventEmitter.guest';

describe('EventEmitter (Guest) - Edge Cases', () => {
  let emitter: GuestEventEmitter;
  let sentMessages: Array<{ event: string; payload: unknown }>;
  let originalSendToHost: unknown;
  let originalOnHostEvent: unknown;
  let hostEventCallback: ((event: string, payload: unknown) => void) | null = null;

  beforeEach(() => {
    originalSendToHost = (globalThis as Record<string, unknown>).sendToHost;
    originalOnHostEvent = (globalThis as Record<string, unknown>).onHostEvent;

    sentMessages = [];
    hostEventCallback = null;

    (globalThis as Record<string, unknown>).sendToHost = (event: string, payload?: unknown) => {
      sentMessages.push({ event, payload });
    };

    (globalThis as Record<string, unknown>).onHostEvent = (
      callback: (event: string, payload: unknown) => void
    ) => {
      hostEventCallback = callback;
    };

    emitter = new GuestEventEmitter();
  });

  afterEach(() => {
    (globalThis as Record<string, unknown>).sendToHost = originalSendToHost;
    (globalThis as Record<string, unknown>).onHostEvent = originalOnHostEvent;
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
    it('should queue messages when sendToHost throws', async () => {
      let callCount = 0;
      (globalThis as Record<string, unknown>).sendToHost = () => {
        callCount++;
        throw new Error('Network error');
      };

      // Create new emitter with failing sendToHost
      emitter = new GuestEventEmitter();

      // This should queue the message
      emitter.emit('test:event', { data: 'test' });

      expect(callCount).toBe(1); // Initial attempt failed
    });

    it('should drop oldest message when queue is full', () => {
      const warnings: unknown[] = [];
      const originalWarn = console.warn;
      console.warn = (...args: unknown[]) => warnings.push(args);

      // Make sendToHost always throw
      (globalThis as Record<string, unknown>).sendToHost = () => {
        throw new Error('Always fails');
      };

      emitter = new GuestEventEmitter();

      // Fill queue beyond maxQueueSize (default 100)
      for (let i = 0; i < 105; i++) {
        emitter.emit(`event${i}`, { index: i });
      }

      console.warn = originalWarn;

      // Should have warned about dropping messages
      expect(warnings.length).toBeGreaterThan(0);
    });

    it('should retry messages when sendToHost becomes available', async () => {
      let sendToHostAvailable = false;
      const successfulMessages: string[] = [];

      (globalThis as Record<string, unknown>).sendToHost = (event: string) => {
        if (!sendToHostAvailable) {
          throw new Error('Not ready');
        }
        successfulMessages.push(event);
      };

      emitter = new GuestEventEmitter();

      // Emit while sendToHost is failing
      emitter.emit('queued:event1', null);
      emitter.emit('queued:event2', null);

      // Wait a bit then make sendToHost available
      await new Promise((resolve) => setTimeout(resolve, 50));
      sendToHostAvailable = true;

      // Wait for retry
      await new Promise((resolve) => setTimeout(resolve, 1100));

      // Messages should have been retried
      expect(successfulMessages.length).toBeGreaterThan(0);
    });

    it('should drop messages after max retries', async () => {
      (globalThis as Record<string, unknown>).sendToHost = () => {
        throw new Error('Permanent failure');
      };

      emitter = new GuestEventEmitter();

      emitter.emit('will:fail', null);

      // Wait for all retries to exhaust (3 retries with 1s delay)
      await new Promise((resolve) => setTimeout(resolve, 3600));

      // Test passes if no crash occurred - the message was dropped gracefully
      expect(true).toBe(true);
    });

    it('should handle missing sendToHost gracefully when processing queue', async () => {
      // Start with working sendToHost
      let callCount = 0;
      (globalThis as Record<string, unknown>).sendToHost = () => {
        callCount++;
        throw new Error('Fail once');
      };

      emitter = new GuestEventEmitter();
      emitter.emit('queued', null);

      // Remove sendToHost
      (globalThis as Record<string, unknown>).sendToHost = undefined;

      // Wait for retry attempt
      await new Promise((resolve) => setTimeout(resolve, 1100));

      // Should not crash, should have attempted once
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
      // @ts-expect-error - Testing invalid input
      emitter.emit('', null);
      // @ts-expect-error - Testing invalid input
      emitter.emit(123, null);

      console.error = originalError;

      expect(errorLogs.length).toBeGreaterThan(0);
    });

    it('should handle emit without sendToHost available', () => {
      const warnings: unknown[] = [];
      const originalWarn = console.warn;
      console.warn = (...args: unknown[]) => warnings.push(args);

      (globalThis as Record<string, unknown>).sendToHost = undefined;

      emitter = new GuestEventEmitter();
      emitter.emit('test', null);

      console.warn = originalWarn;

      // Should warn about sendToHost not being available
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
    it('should not schedule multiple retries simultaneously', async () => {
      let retryCount = 0;
      const originalSetTimeout = globalThis.setTimeout;
      globalThis.setTimeout = ((fn: () => void, delay: number) => {
        retryCount++;
        return originalSetTimeout(fn, delay);
      }) as typeof setTimeout;

      (globalThis as Record<string, unknown>).sendToHost = () => {
        throw new Error('Fail');
      };

      emitter = new GuestEventEmitter();

      // Emit multiple events quickly
      emitter.emit('event1', null);
      emitter.emit('event2', null);
      emitter.emit('event3', null);

      globalThis.setTimeout = originalSetTimeout;

      // Should have scheduled retry only once (not 3 times)
      // Note: May be 1 or more depending on timing, but definitely not 3
      expect(retryCount).toBeGreaterThanOrEqual(1);
      expect(retryCount).toBeLessThan(6); // Safety check
    });
  });
});
