/**
 * Timer Management for Engine
 *
 * Manages setTimeout/setInterval in sandbox with proper cleanup
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

export class TimerManager {
  private timeoutMap = new Map<number, ReturnType<typeof setTimeout>>();
  private intervalMap = new Map<number, ReturnType<typeof setInterval>>();
  private timeoutIdCounter = 0;
  private intervalIdCounter = 0;

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
   * Create setTimeout polyfill for sandbox
   * Returns a function that can be injected into sandbox context
   */
  createSetTimeoutPolyfill(): (fn: () => void, delay: number) => number {
    return (fn: () => void, delay: number) => {
      const id = ++this.timeoutIdCounter;
      const handle = this.nativeSetTimeout(() => {
        this.timeoutMap.delete(id);
        try {
          fn();
        } catch (e) {
          const error = e instanceof Error ? e : new Error(String(e));
          this.options.logger.error(
            `[rill:${this.options.engineId}] setTimeout callback error:`,
            error
          );
          // Notify engine about the error
          this.options.onError?.(error);
        }
      }, delay);
      this.timeoutMap.set(id, handle);
      return id;
    };
  }

  /**
   * Create clearTimeout polyfill for sandbox
   */
  createClearTimeoutPolyfill(): (id: number) => void {
    return (id: number) => {
      const handle = this.timeoutMap.get(id);
      if (handle !== undefined) {
        this.nativeClearTimeout(handle);
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
      const handle = this.nativeSetInterval(() => {
        try {
          fn();
        } catch (e) {
          const error = e instanceof Error ? e : new Error(String(e));
          this.options.logger.error(
            `[rill:${this.options.engineId}] setInterval callback error:`,
            error
          );
          // Notify engine about the error
          this.options.onError?.(error);
        }
      }, delay);
      this.intervalMap.set(id, handle);
      return id;
    };
  }

  /**
   * Create clearInterval polyfill for sandbox
   */
  createClearIntervalPolyfill(): (id: number) => void {
    return (id: number) => {
      const handle = this.intervalMap.get(id);
      if (handle !== undefined) {
        this.nativeClearInterval(handle);
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
    for (const handle of this.timeoutMap.values()) {
      this.nativeClearTimeout(handle);
    }
    this.timeoutMap.clear();

    // Clear all intervals
    for (const handle of this.intervalMap.values()) {
      this.nativeClearInterval(handle);
    }
    this.intervalMap.clear();

    if (this.options.debug) {
      this.options.logger.log(`[rill:${this.options.engineId}] Cleared all timers`);
    }
  }

  /**
   * Get timer statistics
   */
  getStats(): { timeouts: number; intervals: number } {
    return {
      timeouts: this.timeoutMap.size,
      intervals: this.intervalMap.size,
    };
  }
}
