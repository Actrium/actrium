/**
 * @keel/runtime
 *
 * Host-side runtime for keel
 * Responsible for sandbox management, operation receiving, and UI rendering
 */

export type { EngineOptions } from './engine';
// Core exports
export { Engine } from './engine';
export type { ComponentMap } from './registry';

// Sandbox provider selection is internal and automatic (DefaultProvider / TenantManagerProvider).

// Pre-rename spellings of the five names below, kept as `@deprecated` aliases.
export * from './native-guest/deprecated-aliases';
export type {
  NativeComputeRequest,
  NativeGuestComputePoolOptions,
} from './native-guest/native-compute-pool';
export { NativeGuestComputePool } from './native-guest/native-compute-pool';
export type { NativeGuestHostOptions } from './native-guest/native-guest-host';
// Native (non-JS) WASM guest: the low-level host + an EngineViewEngine adapter so
// the platform mounts a native guest through the same useEngineView pipeline.
export { NativeGuestHost } from './native-guest/native-guest-host';
export type { NativeGuestViewOptions } from './native-guest/native-guest-view';
export { createNativeGuestEngine, NativeGuestView } from './native-guest/native-guest-view';
export type {
  NativeGuestWorkerKillInfo,
  NativeGuestWorkerOptions,
} from './native-guest/native-worker-view';
export {
  createNativeGuestWorkerEngine,
  NativeGuestWorkerView,
} from './native-guest/native-worker-view';
// EngineView hook for custom EngineView implementations
export type { LoadingState, UseEngineViewOptions, UseEngineViewResult } from './use-engine-view';
export { useEngineView } from './use-engine-view';
