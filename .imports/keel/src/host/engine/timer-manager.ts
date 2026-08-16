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
  /**
   * Clock and host-timer primitives. Production leaves this unset.
   * Tests pass a per-instance backend so scheduling can be driven in virtual
   * time without patching `globalThis`.
   */
  timing?: TimerManagerTiming;
}

/**
 * Host timing backend used for deadline math and native timer arming.
 *
 * `now` must be monotonic and relative (not a wall clock). The default is
 * `performance.now()` when present, otherwise `Date.now() - origin`.
 */
export interface TimerManagerTiming {
  now: () => number;
  setTimeout: typeof globalThis.setTimeout;
  clearTimeout: typeof globalThis.clearTimeout;
  queueMicrotask: typeof globalThis.queueMicrotask;
}

/** Common fields shared by timeout and interval entries */
interface TimerEntryBase {
  id: number;
  handle: ReturnType<typeof setTimeout> | null;
  remainingTime: number | null;
  pendingToken: number;
}

/** Metadata for a timeout timer */
interface TimeoutEntry extends TimerEntryBase {
  callback: () => void;
  delay: number;
  createdAt: number;
}

/** Metadata for an interval timer */
interface IntervalEntry extends TimerEntryBase {
  callback: () => void;
  delay: number;
  lastTickAt: number;
  /**
   * Absolute time the next tick is due, on a grid anchored at creation
   * (`created + n * delay`). Each tick arms the native timer for
   * `nextTickAt - now` rather than a flat `delay`, so a tick that ran late
   * borrows from the following interval instead of pushing the whole chain back.
   * Without it, every tick's overhead accumulates: measured under an
   * oversubscribed CPU, a 30 ms interval drifted to 99 ms between ticks and
   * delivered one fewer tick per 130 ms than the platform's own setInterval.
   */
  nextTickAt: number;
}

// Sentinel handles for internal scheduling state
const MICROTASK_HANDLE = -1 as unknown as ReturnType<typeof setTimeout>;
const PENDING_HANDLE = -2 as unknown as ReturnType<typeof setTimeout>;

function createMonotonicNow(): { now: () => number; source: string } {
  const performanceObj = globalThis.performance;
  if (performanceObj && typeof performanceObj.now === 'function') {
    return {
      now: () => performanceObj.now(),
      source: 'performance.now',
    };
  }
  const origin = Date.now();
  return {
    now: () => Date.now() - origin,
    source: 'Date.now-origin',
  };
}

export class TimerManager {
  private timeoutMap = new Map<number, TimeoutEntry>();
  private intervalMap = new Map<number, IntervalEntry>();
  private timeoutIdCounter = 0;
  private intervalIdCounter = 0;
  private _isPaused = false;

  /**
   * When set, every guest timer callback is routed through this runner instead
   * of being invoked inline. Timer callbacks are guest-eval ENTRY fired from
   * the host event loop — not from an inbound worker message — so a debug
   * harness that serializes guest entry (the worker's TurnGate) must be able to
   * front them too: while a guest turn is suspended at a breakpoint, a timer
   * callback re-entering the runtime on top of the parked stack is undefined
   * behaviour. Null (the default) means direct invocation — the release path
   * is untouched.
   */
  private callbackRunner: ((run: () => void) => void) | null = null;

  // Native timer references (captured during initialization)
  private nativeSetTimeout: typeof setTimeout;
  private nativeClearTimeout: typeof clearTimeout;
  // Microtask scheduler for zero-delay optimisation
  private nativeQueueMicrotask: (fn: () => void) => void;
  /** Monotonic relative clock used for every deadline. Never `Date.now()`. */
  private readonly now: () => number;

  constructor(private options: TimerManagerOptions) {
    const injected = options.timing;
    let clockSource: string;
    if (injected) {
      this.now = injected.now;
      this.nativeSetTimeout = injected.setTimeout;
      this.nativeClearTimeout = injected.clearTimeout;
      this.nativeQueueMicrotask = injected.queueMicrotask;
      clockSource = 'injected';
    } else {
      const clock = createMonotonicNow();
      this.now = clock.now;
      clockSource = clock.source;
      // Save native timer functions to avoid recursion issues (with fallbacks for test environments)
      const fallbackSetTimeout = ((fn: () => void) => {
        Promise.resolve().then(fn);
        // biome-ignore lint/suspicious/noExplicitAny: Test/internal structure with dynamic types
        return 0 as any;
      }) as typeof setTimeout;

      const fallbackClearTimeout = (() => {}) as typeof clearTimeout;
      // Bind to globalThis: in the browser, window.setTimeout/clearTimeout throw
      // "Illegal invocation" when called with any other `this` (a detached reference).
      // Node/Bun don't care, which is why this only surfaces on the WASM/web path.
      this.nativeSetTimeout =
        typeof globalThis.setTimeout === 'function'
          ? globalThis.setTimeout.bind(globalThis)
          : fallbackSetTimeout;
      this.nativeClearTimeout =
        typeof globalThis.clearTimeout === 'function'
          ? globalThis.clearTimeout.bind(globalThis)
          : fallbackClearTimeout;
      this.nativeQueueMicrotask =
        typeof globalThis.queueMicrotask === 'function'
          ? globalThis.queueMicrotask.bind(globalThis)
          : (fn: () => void) => Promise.resolve().then(fn);
    }

    if (this.options.debug) {
      this.options.logger.log(
        `[TimerManager] Constructor: clock=${clockSource}; globalThis.setTimeout is ${typeof globalThis.setTimeout}`
      );
    }
  }

  /**
   * Install (or clear) the guest-callback runner. See {@link callbackRunner};
   * installed by the worker debug harness so timer-driven guest entry queues
   * behind an outstanding breakpoint suspend.
   */
  setCallbackRunner(runner: ((run: () => void) => void) | null): void {
    this.callbackRunner = runner;
  }

  /** Invoke a guest-callback thunk, through {@link callbackRunner} when installed. */
  private runGuestCallback(run: () => void): void {
    if (this.callbackRunner) {
      this.callbackRunner(run);
    } else {
      run();
    }
  }

  private isMicrotaskHandle(handle: ReturnType<typeof setTimeout> | null): boolean {
    return handle === MICROTASK_HANDLE;
  }

  private isPendingHandle(handle: ReturnType<typeof setTimeout> | null): boolean {
    return handle === PENDING_HANDLE;
  }

  /**
   * Common microtask-based scheduling with pending token invalidation.
   *
   * Pattern shared by setTimeout and setInterval scheduling:
   * 1. Increment pendingToken to invalidate any stale microtask
   * 2. Set handle = PENDING_HANDLE (marker)
   * 3. Queue microtask that checks token validity + paused state
   * 4. If still valid, call onSchedule to set up the native timer
   */
  private scheduleWithPendingToken<E extends TimerEntryBase>(
    entry: E,
    entryMap: Map<number, E>,
    pauseTimeRef: number,
    delay: number,
    onSchedule: (current: E) => void
  ): void {
    const token = ++entry.pendingToken;
    entry.handle = PENDING_HANDLE;

    this.nativeQueueMicrotask(() => {
      const current = entryMap.get(entry.id);
      if (!current || current.pendingToken !== token) return;
      if (this._isPaused) {
        const elapsed = this.now() - pauseTimeRef;
        current.remainingTime = Math.max(0, delay - elapsed);
        current.handle = null;
        return;
      }
      onSchedule(current as E);
    });
  }

  private scheduleNativeTimeout(entry: TimeoutEntry): void {
    this.scheduleWithPendingToken(
      entry,
      this.timeoutMap,
      entry.createdAt,
      entry.delay,
      (current) => {
        current.handle = this.nativeSetTimeout(() => {
          this.timeoutMap.delete(entry.id);
          this.runGuestCallback(() => this.executeCallback(current.callback, 'setTimeout'));
        }, entry.delay);
      }
    );
  }

  private scheduleIntervalTick(entry: IntervalEntry, delay: number): void {
    this.scheduleWithPendingToken(
      entry,
      this.intervalMap as Map<number, IntervalEntry>,
      entry.lastTickAt,
      delay,
      (current) => this.armIntervalTimer(current, delay)
    );
  }

  /**
   * Arm the native timer for one interval tick, and re-arm from inside the tick.
   *
   * Split out of {@link scheduleIntervalTick} so the re-arm can skip the
   * microtask hop that scheduling goes through. That hop exists to let a
   * `clearInterval` or `pause` land before the native timer is created, and it
   * earns its place at creation and resume. On the re-arm path it is pure cost:
   * we are already inside the tick callback, having just checked both
   * `intervalMap.has(id)` and `_isPaused` synchronously, and arming is itself
   * synchronous — so there is no window for either to sneak in. Under a busy
   * event loop the microtask queue drains late, which delayed the moment the
   * next `delay` even STARTED counting; that was the single largest contributor
   * to interval lag (measured: one fewer tick per 130 ms than the platform's own
   * setInterval, under an oversubscribed CPU).
   */
  private armIntervalTimer(entry: IntervalEntry, delay: number): void {
    // Bump the token even though this path queues no microtask: pause() relies on
    // it to invalidate anything still in flight.
    entry.pendingToken++;

    // Cancel whatever this entry is currently armed on before overwriting the
    // handle, or that timer's id is lost and NOTHING can stop it firing.
    //
    // The token above only invalidates microtasks that have not run yet, so it
    // cannot cover a native timer that a microtask already created. That gap is
    // reachable whenever a callbackRunner (the worker debug harness's TurnGate)
    // holds a tick: the native timer has fired, but `entry.handle` still names
    // it, so pause() clears an already-fired timer, resume() arms a fresh one --
    // and then the gate releases the held tick, which re-arms straight from here
    // and orphans the one resume created. Both fire, each re-arms, and the
    // interval doubles on every pause/resume (issue #107).
    //
    // On the release path this is a no-op clear of the timer that just fired.
    // The sentinels are not native ids and must never be passed to clearTimeout.
    if (
      entry.handle !== null &&
      !this.isPendingHandle(entry.handle) &&
      !this.isMicrotaskHandle(entry.handle)
    ) {
      this.nativeClearTimeout(entry.handle);
    }

    // The WHOLE tick body goes through the guest-callback runner, not just
    // the callback: rescheduling only after the (possibly gate-deferred)
    // callback actually ran means a debugger suspend defers ticks one at a
    // time instead of piling up a burst behind the gate.
    const handle = this.nativeSetTimeout(() => {
      this.runGuestCallback(() => {
        const firedAt = this.now();
        entry.lastTickAt = firedAt;

        try {
          entry.callback();
        } catch (e) {
          const error = e instanceof Error ? e : new Error(String(e));
          this.options.logger.error(
            `[keel:${this.options.engineId}] setInterval callback error:`,
            error
          );
          this.options.onError?.(error);
        }

        if (!this.intervalMap.has(entry.id)) return;
        if (this._isPaused) {
          entry.remainingTime = entry.delay;
          entry.handle = null;
          return;
        }

        // Advance along the grid rather than restarting the clock, so a late
        // tick borrows from the next interval instead of shifting every tick
        // after it. If we have fallen a whole interval or more behind, skip the
        // slots we missed instead of firing a catch-up burst -- the same reason
        // the re-arm waits for the callback in the first place.
        const now = this.now();
        entry.nextTickAt += entry.delay;
        if (entry.nextTickAt <= now) {
          entry.nextTickAt = now + entry.delay;
        }
        const nextDelay = Math.max(0, entry.nextTickAt - now);
        if (this.options.debug) {
          this.options.logger.log(
            `[keel:${this.options.engineId}] interval ${entry.id} next delay=${nextDelay}ms (nextTickAt=${entry.nextTickAt})`
          );
        }
        this.armIntervalTimer(entry, nextDelay);
      });
    }, delay);

    entry.handle = handle as unknown as ReturnType<typeof setInterval>;
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

    const now = this.now();

    // Pause all timeouts - calculate and store remaining time
    for (const entry of this.timeoutMap.values()) {
      if (entry.handle !== null) {
        const elapsed = now - entry.createdAt;
        entry.remainingTime = Math.max(0, entry.delay - elapsed);
        if (!this.isMicrotaskHandle(entry.handle) && !this.isPendingHandle(entry.handle)) {
          this.nativeClearTimeout(entry.handle);
        }
        // For microtask-dispatched timeouts (handle === -1), the microtask callback
        // checks timeoutMap.has(id) — setting handle=null prevents re-clear on resume.
        entry.handle = null;
        entry.pendingToken++;
      }
    }

    // Pause all intervals - calculate remaining time until next tick
    for (const entry of this.intervalMap.values()) {
      if (entry.handle !== null) {
        // Straight off the grid: nextTickAt already accounts for a tick that ran
        // late, where `delay - (now - lastTickAt)` would over-report the wait.
        entry.remainingTime = Math.max(0, entry.nextTickAt - now);
        if (!this.isPendingHandle(entry.handle)) {
          // Intervals use recursive setTimeout, so we must use clearTimeout
          this.nativeClearTimeout(entry.handle);
        }
        entry.handle = null;
        entry.pendingToken++;
      }
    }

    if (this.options.debug) {
      this.options.logger.log(
        `[keel:${this.options.engineId}] Paused ${this.timeoutMap.size} timeouts, ${this.intervalMap.size} intervals`
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

    const now = this.now();

    // Resume all timeouts with remaining time
    for (const entry of this.timeoutMap.values()) {
      if (entry.handle === null && entry.remainingTime !== null) {
        entry.createdAt = now; // Reset creation time for accurate tracking
        entry.delay = entry.remainingTime; // Use remaining time as new delay
        entry.remainingTime = null;
        this.scheduleNativeTimeout(entry);
      }
    }

    // Resume all intervals
    for (const entry of this.intervalMap.values()) {
      if (entry.handle === null && entry.remainingTime !== null) {
        // First, schedule the next tick with remaining time
        const remainingTime = entry.remainingTime;
        entry.remainingTime = null;
        // Re-anchor the grid to the resume: the frozen span is not lag to be
        // caught up on, so the next tick is due `remainingTime` from now and the
        // ones after it follow from there.
        entry.lastTickAt = now;
        entry.nextTickAt = now + remainingTime;
        this.scheduleIntervalTick(entry, remainingTime);
      }
    }

    if (this.options.debug) {
      this.options.logger.log(
        `[keel:${this.options.engineId}] Resumed ${this.timeoutMap.size} timeouts, ${this.intervalMap.size} intervals`
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
      this.options.logger.error(`[keel:${this.options.engineId}] ${source} callback error:`, error);
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
      const now = this.now();

      const entry: TimeoutEntry = {
        id,
        handle: null,
        callback: fn,
        delay,
        createdAt: now,
        remainingTime: null,
        pendingToken: 0,
      };

      // Only create native timer if not paused
      if (!this._isPaused) {
        // OPTIMISATION: For zero/near-zero delays, use queueMicrotask instead of
        // native setTimeout. In XPC ViewBridge service context, native RCTTiming
        // is stalled (~24s delay). queueMicrotask fires instantly.
        if (delay <= 0) {
          // Use a sentinel handle to distinguish microtask-dispatched timeouts
          entry.handle = MICROTASK_HANDLE;
          this.nativeQueueMicrotask(() => {
            if (!this.timeoutMap.has(id)) return; // cleared
            this.timeoutMap.delete(id);
            this.runGuestCallback(() => this.executeCallback(fn, 'setTimeout'));
          });
        } else {
          this.scheduleNativeTimeout(entry);
        }
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
        if (
          entry.handle !== null &&
          !this.isMicrotaskHandle(entry.handle) &&
          !this.isPendingHandle(entry.handle)
        ) {
          this.nativeClearTimeout(entry.handle);
        }
        // For microtask-dispatched timeouts (handle === -1), deletion from the map
        // is sufficient — the microtask callback checks timeoutMap.has(id) before firing.
        this.timeoutMap.delete(id);
        entry.pendingToken++;
      }
    };
  }

  /**
   * Create setInterval polyfill for sandbox
   */
  createSetIntervalPolyfill(): (fn: () => void, delay: number) => number {
    return (fn: () => void, delay: number) => {
      const id = ++this.intervalIdCounter;
      const now = this.now();

      const entry: IntervalEntry = {
        id,
        handle: null,
        callback: fn,
        delay,
        lastTickAt: now,
        nextTickAt: now + delay,
        remainingTime: null,
        pendingToken: 0,
      };

      // Only create native timer if not paused
      if (!this._isPaused) {
        this.scheduleIntervalTick(entry, delay);
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
        if (entry.handle !== null && !this.isPendingHandle(entry.handle)) {
          // Intervals use recursive setTimeout, so we must use clearTimeout
          this.nativeClearTimeout(entry.handle);
        }
        this.intervalMap.delete(id);
        entry.pendingToken++;
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
      if (
        entry.handle !== null &&
        !this.isMicrotaskHandle(entry.handle) &&
        !this.isPendingHandle(entry.handle)
      ) {
        this.nativeClearTimeout(entry.handle);
      }
      entry.pendingToken++;
    }
    this.timeoutMap.clear();

    // Clear all intervals
    for (const entry of this.intervalMap.values()) {
      if (entry.handle !== null && !this.isPendingHandle(entry.handle)) {
        // Intervals use recursive setTimeout, so we must use clearTimeout
        this.nativeClearTimeout(entry.handle);
      }
      entry.pendingToken++;
    }
    this.intervalMap.clear();

    if (this.options.debug) {
      this.options.logger.log(`[keel:${this.options.engineId}] Cleared all timers`);
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
