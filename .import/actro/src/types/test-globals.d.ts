// Test globals provided by Bun test runner
// Minimal declarations to satisfy TypeScript during tsc --noEmit

declare function describe(name: string, fn: () => void): void;
declare function it(name: string, fn: () => void): void;
declare function beforeEach(fn: () => void): void;
declare function afterEach(fn: () => void): void;

declare function expect(actual: unknown): {
  toEqual(expected: unknown): void;
  toBe(expected: unknown): void;
  toBeGreaterThanOrEqual(n: number): void;
  toBeLessThan(n: number): void;
  toContain(s: string): void;
  toHaveLength(n: number): void;
  toBeTruthy(): void;
  toBeFalsy(): void;
  toBeUndefined(): void;
  toBeDefined(): void;
  toBeGreaterThan(n: number): void;
  toHaveBeenCalledWith(...args: unknown[]): void;
  toHaveBeenCalledTimes(n: number): void;
  toContainEqual(expected: unknown): void;
  not: {
    toHaveBeenCalled(): void;
    toBe(expected: unknown): void;
    toThrow(): void;
    toBeNull(): void;
    toContain(s: string): void;
    toContainEqual(expected: unknown): void;
  };
  [key: string]: (...args: unknown[]) => void;
};

declare const vi: {
  fn: <T extends (...args: unknown[]) => unknown>() => T & {
    mock?: { calls: unknown[][] };
    mockClear?: () => void;
  };
  useFakeTimers: () => void;
  advanceTimersByTime: (ms: number) => void;
  restoreAllMocks: () => void;
  [key: string]: unknown;
};
