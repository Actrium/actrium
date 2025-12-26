// Test globals provided by Bun test runner
// Minimal declarations to satisfy TypeScript during tsc --noEmit

declare function describe(name: string, fn: () => void): void;
declare function it(name: string, fn: () => void): void;
declare namespace it {
  function skip(name: string, fn: () => void | Promise<void>): void;
  function only(name: string, fn: () => void | Promise<void>): void;
}
declare function beforeEach(fn: () => void): void;
declare function afterEach(fn: () => void): void;

// bun:test module declaration
declare module 'bun:test' {
  interface MockFunction {
    <T>(fn?: T): T;
    module(name: string, factory: () => unknown): void;
  }
  export const mock: MockFunction;
  export function spyOn<T extends object, K extends keyof T>(
    obj: T,
    method: K
  ): {
    mockImplementation(fn: (...args: unknown[]) => unknown): void;
    mockReturnValue(value: unknown): void;
    mockClear(): void;
    mockRestore(): void;
  };
  export function describe(name: string, fn: () => void): void;
  export function it(name: string, fn: () => void | Promise<void>): void;
  export function test(name: string, fn: () => void | Promise<void>): void;
  export function beforeEach(fn: () => void | Promise<void>): void;
  export function afterEach(fn: () => void | Promise<void>): void;
  export function beforeAll(fn: () => void | Promise<void>): void;
  export function afterAll(fn: () => void | Promise<void>): void;
  export function expect(actual: unknown): {
    toBe(expected: unknown): void;
    toEqual(expected: unknown): void;
    toBeNull(): void;
    toBeUndefined(): void;
    toBeDefined(): void;
    toBeTruthy(): void;
    toBeFalsy(): void;
    toContain(expected: unknown): void;
    toHaveLength(expected: number): void;
    toThrow(expected?: unknown): void;
    not: {
      toBe(expected: unknown): void;
      toEqual(expected: unknown): void;
      toBeNull(): void;
      toThrow(): void;
    };
  };
}

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
