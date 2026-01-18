/**
 * Engine Module Exports
 * Main entry point for the Engine and related utilities
 */

// Export Engine class from parent directory
// (The main Engine class remains in ../engine.ts for now to minimize disruption)
export { Engine } from '../Engine';
export type {
  ActivitySample,
  DiagnosticsCollectorOptions,
  LastBatchInfo,
} from './DiagnosticsCollector';
export { DiagnosticsCollector } from './DiagnosticsCollector';
// Export utility modules (for testing and advanced usage)
export { EventManager } from './events';
export {
  CONSOLE_SETUP_CODE,
  createCommonJSGlobals,
  createReactNativeShim,
  formatArg,
  formatConsoleArgs,
  formatWithPlaceholders,
  RUNTIME_HELPERS_CODE,
} from './SandboxHelpers';
export { DEVTOOLS_SHIM } from './shims';
export type { TimerManagerOptions } from './TimerManager';
// Export refactored modules
export { TimerManager } from './TimerManager';
// Export types and interfaces
// Export EngineOptions from types
export type {
  EngineActivityStats,
  EngineActivityTimeline,
  EngineActivityTimelinePoint,
  EngineDiagnostics,
  EngineEvents,
  EngineHealth,
  EngineOptions,
  EventListener,
  GuestMessage,
  IEngine,
  JSEngineContext,
  JSEngineProvider,
  JSEngineRuntime,
  JSEngineRuntimeOptions,
} from './types';
// Export error classes
export { ExecutionError, RequireError, TimeoutError } from './types';
