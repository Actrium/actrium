/**
 * Sandbox compatibility helpers for guest-bundle code.
 *
 * Guest sandboxes (iOS JSC, Android Hermes, QuickJS native/wasm) provide only
 * core ECMAScript intrinsics — web platform globals like `performance` are NOT
 * guaranteed. Everything in the guest bundle that needs them must go through
 * these call-time-checked helpers instead of touching the globals directly.
 * Checks are deliberately performed on every call (not at module eval) so a
 * host that injects the globals after bundle load is picked up, and so tests
 * can simulate a bare sandbox by deleting them.
 */

interface MaybePerformance {
  performance?: { now?: () => number };
}

/**
 * Monotonic-ish milliseconds: `performance.now()` when the sandbox has it,
 * `Date.now()` otherwise. Fallback resolution is ~1ms — fine for the
 * diagnostics/stats callers this is meant for; do not use for wire data.
 */
export function nowMs(): number {
  const p = (globalThis as MaybePerformance).performance;
  return p && typeof p.now === 'function' ? p.now() : Date.now();
}
