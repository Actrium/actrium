/**
 * Throttle and Debounce utilities for event handling
 *
 * Helps prevent performance issues from high-frequency events
 */

/**
 * Throttle function - limits execution to once per time period
 *
 * @param fn - Function to throttle
 * @param delay - Minimum delay between executions (in ms)
 * @returns Throttled function
 *
 * @example
 * ```ts
 * const throttled = throttle(() => {
 *   console.log('Executed');
 * }, 100);
 *
 * // Called multiple times, but executes max once per 100ms
 * throttled();
 * throttled();
 * throttled();
 * ```
 */
export function throttle<T extends (...args: unknown[]) => unknown>(
  fn: T,
  delay: number
): (...args: Parameters<T>) => void {
  let lastCall = 0;
  let timeoutId: ReturnType<typeof setTimeout> | null = null;

  return function throttled(...args: Parameters<T>) {
    const now = Date.now();
    const timeSinceLastCall = now - lastCall;

    // Clear pending timeout if any
    if (timeoutId !== null) {
      clearTimeout(timeoutId);
      timeoutId = null;
    }

    if (timeSinceLastCall >= delay) {
      // Execute immediately if enough time has passed
      lastCall = now;
      fn(...args);
    } else {
      // Schedule execution for later
      const remainingTime = delay - timeSinceLastCall;
      timeoutId = setTimeout(() => {
        lastCall = Date.now();
        fn(...args);
        timeoutId = null;
      }, remainingTime);
    }
  };
}

/**
 * Debounce function - delays execution until after calls have stopped
 *
 * @param fn - Function to debounce
 * @param delay - Delay after last call (in ms)
 * @returns Debounced function
 *
 * @example
 * ```ts
 * const debounced = debounce(() => {
 *   console.log('Executed');
 * }, 100);
 *
 * // Called multiple times rapidly, but executes only once 100ms after last call
 * debounced();
 * debounced();
 * debounced();
 * ```
 */
export function debounce<T extends (...args: unknown[]) => unknown>(
  fn: T,
  delay: number
): (...args: Parameters<T>) => void {
  let timeoutId: ReturnType<typeof setTimeout> | null = null;

  return function debounced(...args: Parameters<T>) {
    // Clear previous timeout
    if (timeoutId !== null) {
      clearTimeout(timeoutId);
    }

    // Schedule new execution
    timeoutId = setTimeout(() => {
      fn(...args);
      timeoutId = null;
    }, delay);
  };
}

/**
 * Configuration for event rate limiting
 */
export interface RateLimitConfig {
  /**
   * Type of rate limiting
   * - 'throttle': Limit to once per period
   * - 'debounce': Wait for calls to stop
   * - 'none': No rate limiting
   */
  type: 'throttle' | 'debounce' | 'none';

  /**
   * Delay in milliseconds (default: 100ms)
   * Keep this low to maintain responsiveness
   */
  delay?: number;
}

/**
 * Create a rate-limited version of a function
 *
 * @param fn - Function to rate limit
 * @param config - Rate limit configuration
 * @returns Rate-limited function
 *
 * @example
 * ```ts
 * const limited = rateLimit(
 *   () => console.log('Event'),
 *   { type: 'throttle', delay: 100 }
 * );
 * ```
 */
export function rateLimit<T extends (...args: unknown[]) => unknown>(
  fn: T,
  config: RateLimitConfig
): (...args: Parameters<T>) => void {
  const delay = config.delay ?? 100; // Default 100ms - not too high

  switch (config.type) {
    case 'throttle':
      return throttle(fn, delay);
    case 'debounce':
      return debounce(fn, delay);
    case 'none':
    default:
      return fn as (...args: Parameters<T>) => void;
  }
}
