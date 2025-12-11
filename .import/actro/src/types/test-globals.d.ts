// Test globals provided by Bun test runner
// Minimal declarations to satisfy TypeScript during tsc --noEmit

declare function describe(name: string, fn: () => void): void;
declare function it(name: string, fn: () => void): void;
declare function beforeEach(fn: () => void): void;
declare function afterEach(fn: () => void): void;

declare function expect(actual: unknown): any;

declare const vi: any; // test doubles (if present)
