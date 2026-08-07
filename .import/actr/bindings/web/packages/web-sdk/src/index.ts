/**
 * Actor-RTC Web SDK
 *
 * High-level TypeScript SDK for Actor-RTC Web platform
 *
 * WASM-DOM architecture:
 * - DOM layer: @actrium/actr-dom (HAL)
 * - Service Worker layer: hosts WASM runtime
 * - UI layer: SDK wraps WASM for application use
 */

export * from './actor';
export * from './config';
export * from './errors';
export * from './types';
export * from './actor-ref';
// Package inspection only. The explicitly named APIs return unverified data;
// historical short names remain deprecated for source compatibility.
export * from './package-loader';

// Unified API
export { Actor, createActor } from './actor';
export type { ActorConfig } from './actor';
export {
  loadUnverifiedActrPackage,
  parseUnverifiedActrPackage,
  loadActrPackage,
  parseActrPackage,
} from './package-loader';
export type {
  UnverifiedActrManifest,
  UnverifiedLoadedActrPackage,
  ActrManifest,
  LoadedActrPackage,
} from './package-loader';
