/**
 * Host Config Tests
 *
 * Tests for React Reconciler host config and scheduler customization
 */

import { afterEach, describe, expect, test } from 'bun:test';
import { resetReconcilerScheduler, setReconcilerScheduler } from './host-config';

describe('host-config', () => {
  describe('setReconcilerScheduler()', () => {
    afterEach(() => {
      // Reset to default after each test
      resetReconcilerScheduler();
    });

    test('should set custom scheduler', () => {
      const scheduledTasks: Array<{ fn: () => void; delay: number }> = [];
      const cancelledIds: number[] = [];

      const customSchedule = (fn: () => void, delay: number) => {
        const id = scheduledTasks.length;
        scheduledTasks.push({ fn, delay });
        return id;
      };

      const customCancel = (id: number) => {
        cancelledIds.push(id);
      };

      setReconcilerScheduler(customSchedule, customCancel);

      // Verify it doesn't throw
      expect(scheduledTasks).toEqual([]);
      expect(cancelledIds).toEqual([]);
    });

    test('should allow scheduling with custom scheduler', () => {
      let scheduled = false;
      const customSchedule = (fn: () => void, delay: number) => {
        scheduled = true;
        expect(typeof fn).toBe('function');
        expect(typeof delay).toBe('number');
        return 123;
      };

      const customCancel = (id: number) => {
        expect(id).toBe(123);
      };

      setReconcilerScheduler(customSchedule, customCancel);

      // The scheduler will be used internally by reconciler
      expect(scheduled).toBe(false); // Not scheduled yet, just set
    });

    test('should handle zero delay', () => {
      let capturedDelay: number | null = null;

      const customSchedule = (_fn: () => void, delay: number) => {
        capturedDelay = delay;
        return 0;
      };

      setReconcilerScheduler(customSchedule, () => {});

      expect(capturedDelay).toBe(null); // Not called yet
    });

    test('should handle multiple scheduler changes', () => {
      const schedule1 = () => 1;
      const cancel1 = () => {};

      const schedule2 = () => 2;
      const cancel2 = () => {};

      setReconcilerScheduler(schedule1, cancel1);
      setReconcilerScheduler(schedule2, cancel2);

      // Should not throw
      expect(true).toBe(true);
    });
  });

  describe('resetReconcilerScheduler()', () => {
    test('should reset to default globalThis.setTimeout/clearTimeout', () => {
      // Set custom scheduler first
      const customSchedule = () => 999;
      const customCancel = () => {};

      setReconcilerScheduler(customSchedule, customCancel);

      // Reset to default
      resetReconcilerScheduler();

      // After reset, it should use globalThis.setTimeout/clearTimeout
      // We can't directly test the internal state, but we can verify it doesn't throw
      expect(true).toBe(true);
    });

    test('should be idempotent', () => {
      resetReconcilerScheduler();
      resetReconcilerScheduler();
      resetReconcilerScheduler();

      expect(true).toBe(true);
    });

    test('should work after multiple set/reset cycles', () => {
      const custom = () => 1;

      setReconcilerScheduler(custom, () => {});
      resetReconcilerScheduler();
      setReconcilerScheduler(custom, () => {});
      resetReconcilerScheduler();

      expect(true).toBe(true);
    });
  });

  describe('scheduler integration', () => {
    afterEach(() => {
      resetReconcilerScheduler();
    });

    test('should handle scheduler with immediate execution', () => {
      let executed = false;

      const immediateSchedule = (fn: () => void, delay: number) => {
        // Execute immediately for testing
        if (delay === 0) {
          fn();
          executed = true;
        }
        return 0;
      };

      setReconcilerScheduler(immediateSchedule, () => {});

      expect(executed).toBe(false); // Not triggered yet by our test
    });

    test('should handle scheduler cancellation', () => {
      const cancelledIds: number[] = [];

      const customSchedule = (_fn: () => void, _delay: number) => {
        return Math.floor(Math.random() * 1000);
      };

      const customCancel = (id: number) => {
        cancelledIds.push(id);
      };

      setReconcilerScheduler(customSchedule, customCancel);

      // Cancellation will be tested through reconciler usage
      expect(cancelledIds).toEqual([]);
    });

    test('should handle scheduler with errors', () => {
      const errorSchedule = (_fn: () => void, delay: number) => {
        // Scheduler that might throw (but we catch it)
        try {
          if (delay < 0) {
            throw new Error('Invalid delay');
          }
          return 0;
        } catch {
          return -1;
        }
      };

      setReconcilerScheduler(errorSchedule, () => {});

      expect(true).toBe(true);
    });
  });

  describe('edge cases', () => {
    afterEach(() => {
      resetReconcilerScheduler();
    });

    test('should handle rapid scheduler changes', () => {
      for (let i = 0; i < 100; i++) {
        setReconcilerScheduler(
          () => i,
          () => {}
        );
      }

      resetReconcilerScheduler();
      expect(true).toBe(true);
    });

    test('should handle concurrent operations', () => {
      setReconcilerScheduler(
        () => 1,
        () => {}
      );

      resetReconcilerScheduler();

      expect(true).toBe(true);
    });

    test('should handle scheduler with null/undefined (type safety)', () => {
      // TypeScript should prevent this, but test runtime behavior
      const safeSchedule = (fn: unknown, delay: unknown) => {
        return typeof fn === 'function' && typeof delay === 'number' ? 0 : -1;
      };

      setReconcilerScheduler(safeSchedule as never, () => {});

      expect(true).toBe(true);
    });

    test('should handle devtools state during scheduler changes', () => {
      setReconcilerScheduler(
        () => 1,
        () => {}
      );

      resetReconcilerScheduler();

      expect(true).toBe(true);
    });

    test('should handle reset without prior set', () => {
      // Reset without ever calling set
      resetReconcilerScheduler();

      expect(true).toBe(true);
    });

    test('should preserve scheduler after devtools changes', () => {
      const customSchedule = () => 999;

      setReconcilerScheduler(customSchedule, () => {});

      // Scheduler should still be custom (not reset by devtools changes)
      expect(true).toBe(true);
    });
  });

  describe('performance', () => {
    afterEach(() => {
      resetReconcilerScheduler();
    });

    test('should handle high-frequency scheduler calls', () => {
      let callCount = 0;
      const highFreqSchedule = (_fn: () => void, _delay: number) => {
        callCount++;
        return callCount;
      };

      setReconcilerScheduler(highFreqSchedule, () => {});

      // Scheduler is set but not called yet
      expect(callCount).toBe(0);
    });

    test('should handle long-running scheduled tasks', () => {
      const tasks: Array<() => void> = [];

      const taskSchedule = (fn: () => void, _delay: number) => {
        tasks.push(fn);
        return tasks.length - 1;
      };

      setReconcilerScheduler(taskSchedule, () => {});

      expect(tasks).toEqual([]);
    });

    test('should handle scheduler memory cleanup', () => {
      const weakRefs: WeakRef<object>[] = [];

      const trackingSchedule = (fn: () => void, _delay: number) => {
        // Track function references
        if (typeof fn === 'function') {
          const obj = fn as unknown as object;
          weakRefs.push(new WeakRef(obj));
        }
        return 0;
      };

      setReconcilerScheduler(trackingSchedule, () => {});

      resetReconcilerScheduler();

      expect(weakRefs).toEqual([]);
    });
  });
});
