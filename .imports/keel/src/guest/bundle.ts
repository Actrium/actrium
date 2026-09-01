/**
 * Guest Bundle - Unified Entry Point
 *
 * This is the single entry point for all Guest sandbox code.
 * Build script compiles this into a single bundle that is eval'd once.
 *
 * Import order is critical:
 * 1. init.ts - Sets up __KEEL_GUEST_ENV__ and __keel.callbacks FIRST
 * 2. globals-setup.ts - Sets up console and runtime helpers
 * 3. react-global.ts - Exposes React/JSX runtimes on globalThis
 * 4. sdk - Exposes keel/guest as globalThis.KeelGuest
 * 5. reconciler - KeelReconciler (uses React)
 *
 * Output:
 * - globalThis.React - React API
 * - globalThis.ReactJSXRuntime - JSX runtime for modern transform
 * - globalThis.ReactJSXDevRuntime - JSX dev runtime
 * - globalThis.KeelGuest - keel/guest module exports (for externalized guest bundles)
 * - globalThis.KeelReconciler - Reconciler API (render, unmount, etc.)
 * - globalThis.__REACT_SHIM__ - Marker that shims are loaded
 * - globalThis.console - Console object (wraps Host callbacks)
 * - globalThis.__keel_onHostEvent - Subscribe to Host events
 * - globalThis.__keel.dispatchEvent - Called by Host to dispatch events
 */

// ============================================
// 1. Guest Environment Initialization (MUST BE FIRST)
// ============================================
import './runtime/init';

// ============================================
// 2. Console and Runtime Helpers Setup
// ============================================
import './runtime/globals-setup';

// ============================================
// 3. React globals
// ============================================
import './runtime/react-global';

// ============================================
// 4. Guest SDK (keel/guest)
// ============================================
import * as KeelGuestModule from '../sdk';

(globalThis as Record<string, unknown>).KeelGuest = KeelGuestModule;

// ============================================
// 5. Reconciler
// ============================================
import {
  getCallbackCount,
  invokeCallback,
  registerComponentType,
  releaseCallback,
  render,
  unmount,
  unmountAll,
  unregisterComponentTypes,
} from './runtime/reconciler';

// Export KeelReconciler to globalThis
(globalThis as Record<string, unknown>).KeelReconciler = {
  render,
  unmount,
  unmountAll,
  invokeCallback,
  releaseCallback,
  getCallbackCount,
  registerComponentType,
  unregisterComponentTypes,
};

// ============================================
// Debug logging
// ============================================
if ((globalThis as Record<string, unknown>).__KEEL_DEBUG__) {
  console.log('[keel:guest-bundle] Guest bundle initialized');
  console.log('[keel:guest-bundle] - React:', typeof (globalThis as Record<string, unknown>).React);
  console.log(
    '[keel:guest-bundle] - ReactJSXRuntime:',
    typeof (globalThis as Record<string, unknown>).ReactJSXRuntime
  );
  console.log(
    '[keel:guest-bundle] - KeelGuest:',
    typeof (globalThis as Record<string, unknown>).KeelGuest
  );
  console.log(
    '[keel:guest-bundle] - KeelReconciler:',
    typeof (globalThis as Record<string, unknown>).KeelReconciler
  );
}

// Export marker for build verification
export const GUEST_BUNDLE_COMPLETE = true;
