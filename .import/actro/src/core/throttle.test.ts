/**
 * Throttle and Debounce Tests
 */

import { throttle, debounce, rateLimit } from './throttle';

describe('Throttle', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('should execute immediately on first call', () => {
    const fn = vi.fn();
    const throttled = throttle(fn, 100);

    throttled();

    expect(fn).toHaveBeenCalledTimes(1);
  });

  it('should throttle rapid calls', () => {
    const fn = vi.fn();
    const throttled = throttle(fn, 100);

    throttled();
    throttled();
    throttled();

    expect(fn).toHaveBeenCalledTimes(1);
  });

  it('should execute after delay period', () => {
    const fn = vi.fn();
    const throttled = throttle(fn, 100);

    throttled(); // t=0, executes immediately
    expect(fn).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(50);
    throttled(); // t=50, schedules for t=100

    expect(fn).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(50); // t=100
    expect(fn).toHaveBeenCalledTimes(2);
  });

  it('should pass arguments correctly', () => {
    const fn = vi.fn();
    const throttled = throttle(fn, 100);

    throttled('arg1', 'arg2', 123);

    expect(fn).toHaveBeenCalledWith('arg1', 'arg2', 123);
  });

  it('should handle multiple throttle windows', () => {
    const fn = vi.fn();
    const throttled = throttle(fn, 100);

    throttled(); // t=0
    expect(fn).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(150); // t=150
    throttled(); // executes immediately (>100ms since last call)
    expect(fn).toHaveBeenCalledTimes(2);

    vi.advanceTimersByTime(150); // t=300
    throttled(); // executes immediately
    expect(fn).toHaveBeenCalledTimes(3);
  });

  it('should clear pending timeout on new call within throttle period', () => {
    const fn = vi.fn();
    const throttled = throttle(fn, 100);

    throttled(); // t=0, executes
    expect(fn).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(50); // t=50
    throttled(); // schedules for t=100

    vi.advanceTimersByTime(25); // t=75
    throttled(); // cancels previous, schedules for t=100 (75+25)

    vi.advanceTimersByTime(25); // t=100
    expect(fn).toHaveBeenCalledTimes(2);
  });
});

describe('Debounce', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('should not execute immediately', () => {
    const fn = vi.fn();
    const debounced = debounce(fn, 100);

    debounced();

    expect(fn).toHaveBeenCalledTimes(0);
  });

  it('should execute after delay', () => {
    const fn = vi.fn();
    const debounced = debounce(fn, 100);

    debounced();
    expect(fn).toHaveBeenCalledTimes(0);

    vi.advanceTimersByTime(100);
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it('should reset timer on rapid calls', () => {
    const fn = vi.fn();
    const debounced = debounce(fn, 100);

    debounced(); // t=0
    vi.advanceTimersByTime(50); // t=50
    debounced(); // resets timer
    vi.advanceTimersByTime(50); // t=100 (but timer was reset at t=50)

    expect(fn).toHaveBeenCalledTimes(0);

    vi.advanceTimersByTime(50); // t=150 (100ms since last call)
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it('should pass arguments correctly', () => {
    const fn = vi.fn();
    const debounced = debounce(fn, 100);

    debounced('arg1', 'arg2', 123);
    vi.advanceTimersByTime(100);

    expect(fn).toHaveBeenCalledWith('arg1', 'arg2', 123);
  });

  it('should use last call arguments', () => {
    const fn = vi.fn();
    const debounced = debounce(fn, 100);

    debounced('first');
    vi.advanceTimersByTime(50);
    debounced('second');
    vi.advanceTimersByTime(50);
    debounced('third');
    vi.advanceTimersByTime(100);

    expect(fn).toHaveBeenCalledTimes(1);
    expect(fn).toHaveBeenCalledWith('third');
  });

  it('should allow multiple execution windows', () => {
    const fn = vi.fn();
    const debounced = debounce(fn, 100);

    debounced('first');
    vi.advanceTimersByTime(100);
    expect(fn).toHaveBeenCalledTimes(1);
    expect(fn).toHaveBeenCalledWith('first');

    debounced('second');
    vi.advanceTimersByTime(100);
    expect(fn).toHaveBeenCalledTimes(2);
    expect(fn).toHaveBeenCalledWith('second');
  });
});

describe('rateLimit', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('should create throttled function when type is throttle', () => {
    const fn = vi.fn();
    const limited = rateLimit(fn, { type: 'throttle', delay: 100 });

    limited();
    limited();

    expect(fn).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(100);
    expect(fn).toHaveBeenCalledTimes(2);
  });

  it('should create debounced function when type is debounce', () => {
    const fn = vi.fn();
    const limited = rateLimit(fn, { type: 'debounce', delay: 100 });

    limited();
    expect(fn).toHaveBeenCalledTimes(0);

    vi.advanceTimersByTime(100);
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it('should return original function when type is none', () => {
    const fn = vi.fn();
    const limited = rateLimit(fn, { type: 'none' });

    limited();
    limited();
    limited();

    expect(fn).toHaveBeenCalledTimes(3);
  });

  it('should use default delay of 100ms', () => {
    const fn = vi.fn();
    const limited = rateLimit(fn, { type: 'throttle' });

    limited();
    expect(fn).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(99);
    limited();
    expect(fn).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(1); // total 100ms
    expect(fn).toHaveBeenCalledTimes(2);
  });

  it('should accept custom delay', () => {
    const fn = vi.fn();
    const limited = rateLimit(fn, { type: 'throttle', delay: 200 });

    limited();
    expect(fn).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(150);
    limited();
    expect(fn).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(50); // total 200ms
    expect(fn).toHaveBeenCalledTimes(2);
  });
});
