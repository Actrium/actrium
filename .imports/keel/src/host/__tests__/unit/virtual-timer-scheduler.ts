/**
 * Deterministic host-timer backend for TimerManager tests.
 *
 * Virtual monotonic time, explicit delivery, no host sleeps, no global
 * monkey-patching. Each instance is isolated so neighbouring files cannot
 * steal or delay its timers.
 */

import type { TimerManagerTiming } from '../../engine/timer-manager';

interface ScheduledTimer {
  dueAt: number;
  delay: number;
  callback: () => void;
}

export class VirtualTimerScheduler {
  private virtualNow = 0;
  private nextId = 1;
  private readonly timers = new Map<number, ScheduledTimer>();
  private readonly microtasks: Array<() => void> = [];
  readonly requestedDelays: number[] = [];

  readonly timing: TimerManagerTiming = {
    now: () => this.virtualNow,
    setTimeout: ((callback: () => void, delay?: number) => {
      const ms = typeof delay === 'number' && Number.isFinite(delay) ? delay : 0;
      const id = this.nextId++;
      this.requestedDelays.push(ms);
      this.timers.set(id, {
        dueAt: this.virtualNow + ms,
        delay: ms,
        callback,
      });
      return id;
    }) as typeof setTimeout,
    clearTimeout: ((id: ReturnType<typeof setTimeout>) => {
      this.timers.delete(id as unknown as number);
    }) as typeof clearTimeout,
    queueMicrotask: (fn: () => void) => {
      this.microtasks.push(fn);
    },
  };

  get now(): number {
    return this.virtualNow;
  }

  get liveTimerCount(): number {
    return this.timers.size;
  }

  get liveTimerIds(): number[] {
    return [...this.timers.keys()];
  }

  get lastRequestedDelay(): number | undefined {
    return this.requestedDelays[this.requestedDelays.length - 1];
  }

  flushMicrotasks(): void {
    while (this.microtasks.length > 0) {
      const batch = this.microtasks.splice(0);
      for (const fn of batch) {
        fn();
      }
    }
  }

  /**
   * Fire the earliest live native timer. `at` delivers it late (or on time)
   * without auto-firing anything else.
   */
  deliverNext(at?: number): void {
    if (this.timers.size === 0) {
      throw new Error('VirtualTimerScheduler.deliverNext: no live timers');
    }
    let nextId = Number.POSITIVE_INFINITY;
    let nextDue = Number.POSITIVE_INFINITY;
    for (const [id, timer] of this.timers) {
      if (timer.dueAt < nextDue || (timer.dueAt === nextDue && id < nextId)) {
        nextDue = timer.dueAt;
        nextId = id;
      }
    }
    const timer = this.timers.get(nextId);
    if (!timer) {
      throw new Error('VirtualTimerScheduler.deliverNext: timer disappeared');
    }
    const fireAt = at ?? timer.dueAt;
    if (fireAt < this.virtualNow) {
      throw new Error(
        `VirtualTimerScheduler.deliverNext: cannot deliver at ${fireAt} before current time ${this.virtualNow}`
      );
    }
    this.virtualNow = fireAt;
    this.timers.delete(nextId);
    timer.callback();
  }

  /** Advance virtual time without firing timers. Used inside a guest callback. */
  advance(ms: number): void {
    if (ms < 0) {
      throw new Error(`VirtualTimerScheduler.advance: negative delta ${ms}`);
    }
    this.virtualNow += ms;
  }
}
