/**
 * Timer Management for Engine
 *
 * Manages setTimeout/setInterval in sandbox with proper cleanup
 * Supports pause/resume with true clock freezing
 */

export interface TimerManagerOptions {
  debug: boolean;
  logger: {
    log: (...args: unknown[]) => void;
    error: (...args: unknown[]) => void;
  };
  engineId: string;
  onError?: (error: Error) => void;
}

/** Metadata for a timeout timer */
interface TimeoutEntry {
  id: number;
  handle: ReturnType<typeof setTimeout> | null;
  callback: () => void;
  delay: number;
  createdAt: number;
  remainingTime: number | null; // Set when paused
}

/** Metadata for an interval timer */
interface IntervalEntry {
  id: number;
  handle: ReturnType<typeof setInterval> | null;
  callback: () => void;
  delay: number;
  lastTickAt: number;
  remainingTime: number | null; // Set when paused
}

export class TimerManager {
  private timeoutMap = new Map<number, TimeoutEntry>();
  private intervalMap = new Map<number, IntervalEntry>();
  private timeoutIdCounter = 0;
  private intervalIdCounter = 0;
  private _isPaused = false;

  // Native timer references (captured during initialization)
  private nativeSetTimeout: typeof setTimeout;
  private nativeClearTimeout: typeof clearTimeout;
  private nativeSetInterval: typeof setInterval;
  private nativeClearInterval: typeof clearInterval;

  constructor(private options: TimerManagerOptions) {
    // Save native timer functions to avoid recursion issues (with fallbacks for test environments)
    const fallbackSetTimeout = ((fn: () => void) => {
      Promise.resolve().then(fn);
      // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
      return 0 as any;
    }) as typeof setTimeout;

    const fallbackClearTimeout = (() => {}) as typeof clearTimeout;
    // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
    const fallbackSetInterval = (() => 0 as any) as typeof setInterval;

    const fallbackClearInterval = (() => {}) as typeof clearInterval;

    this.nativeSetTimeout =
      typeof globalThis.setTimeout === 'function'
        ? globalThis.setTimeout.bind(globalThis)
        : fallbackSetTimeout;
    this.nativeClearTimeout =
      typeof globalThis.clearTimeout === 'function'
        ? globalThis.clearTimeout.bind(globalThis)
        : fallbackClearTimeout;
    this.nativeSetInterval =
      typeof globalThis.setInterval === 'function'
        ? globalThis.setInterval.bind(globalThis)
        : fallbackSetInterval;
    this.nativeClearInterval =
      typeof globalThis.clearInterval === 'function'
        ? globalThis.clearInterval.bind(globalThis)
        : fallbackClearInterval;
  }

  /**
   * Whether the timer manager is currently paused
   */
  get isPaused(): boolean {
    return this._isPaused;
  }

  /**
   * Pause all timers - freeze their clocks
   * Stores remaining time for each timer and clears native handles
   */
  pause(): void {
    if (this._isPaused) return;
    this._isPaused = true;

    const now = Date.now();

    // Pause all timeouts - calculate and store remaining time
    for (const entry of this.timeoutMap.values()) {
      if (entry.handle !== null) {
        const elapsed = now - entry.createdAt;
        entry.remainingTime = Math.max(0, entry.delay - elapsed);
        this.nativeClearTimeout(entry.handle);
        entry.handle = null;
      }
    }

    // Pause all intervals - calculate remaining time until next tick
    for (const entry of this.intervalMap.values()) {
      if (entry.handle !== null) {
        const elapsed = now - entry.lastTickAt;
        entry.remainingTime = Math.max(0, entry.delay - elapsed);
        this.nativeClearInterval(entry.handle);
        entry.handle = null;
      }
    }

    if (this.options.debug) {
      this.options.logger.log(
        `[rill:${this.options.engineId}] Paused ${this.timeoutMap.size} timeouts, ${this.intervalMap.size} intervals`
      );
    }
  }

  /**
   * Resume all timers - continue from where they left off
   * Recreates native handles with remaining time
   */
  resume(): void {
    if (!this._isPaused) return;
    this._isPaused = false;

    const now = Date.now();

    // Resume all timeouts with remaining time
    for (const entry of this.timeoutMap.values()) {
      if (entry.handle === null && entry.remainingTime !== null) {
        entry.createdAt = now; // Reset creation time for accurate tracking
        entry.delay = entry.remainingTime; // Use remaining time as new delay
        entry.remainingTime = null;
        entry.handle = this.nativeSetTimeout(() => {
          this.timeoutMap.delete(entry.id);
          this.executeCallback(entry.callback, 'setTimeout');
        }, entry.delay);
      }
    }

    // Resume all intervals
    for (const entry of this.intervalMap.values()) {
      if (entry.handle === null && entry.remainingTime !== null) {
        // First, schedule the next tick with remaining time
        const remainingTime = entry.remainingTime;
        entry.remainingTime = null;

        // Use setTimeout for the first tick (with remaining time), then switch to setInterval
        const firstTickHandle = this.nativeSetTimeout(() => {
          entry.lastTickAt = Date.now();
          this.executeCallback(entry.callback, 'setInterval');

          // Now start the regular interval
          entry.handle = this.nativeSetInterval(() => {
            entry.lastTickAt = Date.now();
            this.executeCallback(entry.callback, 'setInterval');
          }, entry.delay);
        }, remainingTime);

        // Store the setTimeout handle temporarily (will be replaced by setInterval)
        entry.handle = firstTickHandle as unknown as ReturnType<typeof setInterval>;
      }
    }

    if (this.options.debug) {
      this.options.logger.log(
        `[rill:${this.options.engineId}] Resumed ${this.timeoutMap.size} timeouts, ${this.intervalMap.size} intervals`
      );
    }
  }

  /**
   * Execute a callback with error handling
   */
  private executeCallback(callback: () => void, source: string): void {
    try {
      callback();
    } catch (e) {
      const error = e instanceof Error ? e : new Error(String(e));
      this.options.logger.error(
        `[rill:${this.options.engineId}] ${source} callback error:`,
        error
      );
      this.options.onError?.(error);
    }
  }

  /**
   * Create setTimeout polyfill for sandbox
   * Returns a function that can be injected into sandbox context
   */
  createSetTimeoutPolyfill(): (fn: () => void, delay: number) => number {
    return (fn: () => void, delay: number) => {
      const id = ++this.timeoutIdCounter;
      const now = Date.now();

      const entry: TimeoutEntry = {
        id,
        handle: null,
        callback: fn,
        delay,
        createdAt: now,
        remainingTime: null,
      };

      // Only create native timer if not paused
      if (!this._isPaused) {
        entry.handle = this.nativeSetTimeout(() => {
          this.timeoutMap.delete(id);
          this.executeCallback(fn, 'setTimeout');
        }, delay);
      } else {
        // If paused, store remaining time as the full delay
        entry.remainingTime = delay;
      }

      this.timeoutMap.set(id, entry);
      return id;
    };
  }

  /**
   * Create clearTimeout polyfill for sandbox
   */
  createClearTimeoutPolyfill(): (id: number) => void {
    return (id: number) => {
      const entry = this.timeoutMap.get(id);
      if (entry) {
        if (entry.handle !== null) {
          this.nativeClearTimeout(entry.handle);
        }
        this.timeoutMap.delete(id);
      }
    };
  }

  /**
   * Create setInterval polyfill for sandbox
   */
  createSetIntervalPolyfill(): (fn: () => void, delay: number) => number {
    return (fn: () => void, delay: number) => {
      const id = ++this.intervalIdCounter;
      const now = Date.now();

      const entry: IntervalEntry = {
        id,
        handle: null,
        callback: fn,
        delay,
        lastTickAt: now,
        remainingTime: null,
      };

      // Only create native timer if not paused
      if (!this._isPaused) {
        entry.handle = this.nativeSetInterval(() => {
          entry.lastTickAt = Date.now();
          this.executeCallback(fn, 'setInterval');
        }, delay);
      } else {
        // If paused, store remaining time as the full delay
        entry.remainingTime = delay;
      }

      this.intervalMap.set(id, entry);
      return id;
    };
  }

  /**
   * Create clearInterval polyfill for sandbox
   */
  createClearIntervalPolyfill(): (id: number) => void {
    return (id: number) => {
      const entry = this.intervalMap.get(id);
      if (entry) {
        if (entry.handle !== null) {
          this.nativeClearInterval(entry.handle);
        }
        this.intervalMap.delete(id);
      }
    };
  }

  /**
   * Clear all pending timers (timeouts and intervals)
   * Called during engine cleanup
   */
  clearAllTimers(): void {
    // Clear all timeouts
    for (const entry of this.timeoutMap.values()) {
      if (entry.handle !== null) {
        this.nativeClearTimeout(entry.handle);
      }
    }
    this.timeoutMap.clear();

    // Clear all intervals
    for (const entry of this.intervalMap.values()) {
      if (entry.handle !== null) {
        this.nativeClearInterval(entry.handle);
      }
    }
    this.intervalMap.clear();

    if (this.options.debug) {
      this.options.logger.log(`[rill:${this.options.engineId}] Cleared all timers`);
    }
  }

  /**
   * Get timer statistics
   */
  getStats(): { timeouts: number; intervals: number; isPaused: boolean } {
    return {
      timeouts: this.timeoutMap.size,
      intervals: this.intervalMap.size,
      isPaused: this._isPaused,
    };
  }
}
