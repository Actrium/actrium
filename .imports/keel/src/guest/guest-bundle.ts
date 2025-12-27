/**
 * Guest Bundle - Unified Entry Point
 *
 * This is the single entry point for all Guest sandbox code.
 * Build script compiles this into a single bundle that is eval'd once.
 *
 * Import order is critical:
 * 1. init.ts - Sets up __RILL_GUEST_ENV__ and __callbacks FIRST
 * 2. globals-setup.ts - Sets up console and runtime helpers
 * 3. shims - React/JSX shims (needs console for error logging)
 * 4. reconciler - RillReconciler (uses React shims)
 *
 * Output:
 * - globalThis.React - React API
 * - globalThis.ReactJSXRuntime - JSX runtime for modern transform
 * - globalThis.ReactJSXDevRuntime - JSX dev runtime
 * - globalThis.RillReconciler - Reconciler API (render, unmount, etc.)
 * - globalThis.__REACT_SHIM__ - Marker that shims are loaded
 * - globalThis.console - Console object (wraps Host callbacks)
 * - globalThis.__useHostEvent - Subscribe to Host events
 * - globalThis.__handleHostEvent - Called by Host to dispatch events
 */

// ============================================
// 1. Guest Environment Initialization (MUST BE FIRST)
// ============================================
import './init';

// ============================================
// 2. Console and Runtime Helpers Setup
// ============================================
import './globals-setup';

// ============================================
// 3. React/JSX Shims
// ============================================
import { React, ReactJSXDevRuntime, ReactJSXRuntime } from './shims/react';

// Mark shims as loaded
(globalThis as Record<string, unknown>).__REACT_SHIM__ = true;

// Export React to globalThis
(globalThis as Record<string, unknown>).React = React;
(globalThis as Record<string, unknown>).ReactJSXRuntime = ReactJSXRuntime;
(globalThis as Record<string, unknown>).ReactJSXDevRuntime = ReactJSXDevRuntime;

// ============================================
// 4. Reconciler
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
} from './reconciler';

// Export RillReconciler to globalThis
(globalThis as Record<string, unknown>).RillReconciler = {
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
if ((globalThis as Record<string, unknown>).__RILL_DEBUG__) {
  console.log('[rill:guest-bundle] Guest bundle initialized');
  console.log('[rill:guest-bundle] - React:', typeof React);
  console.log('[rill:guest-bundle] - ReactJSXRuntime:', typeof ReactJSXRuntime);
  console.log(
    '[rill:guest-bundle] - RillReconciler:',
    typeof (globalThis as Record<string, unknown>).RillReconciler
  );
}

// Export marker for build verification
export const GUEST_BUNDLE_COMPLETE = true;
