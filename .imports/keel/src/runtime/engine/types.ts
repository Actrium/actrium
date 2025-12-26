/**
 * Engine Types and Interfaces
 */

import type { RuntimeCollectorConfig } from '../../devtools/runtime';
import type { JSEngineProvider } from '../../sandbox';

/**
 * Engine configuration options
 */
export interface EngineOptions {
  /**
   * JS Engine provider for creating sandbox runtime.
   * Optional - if not provided, a default will be selected based on the environment.
   */
  provider?: JSEngineProvider;

  /**
   * Legacy parameter: `quickjs` (equivalent to `provider`)
   * @deprecated Use `provider` instead
   */
  quickjs?: JSEngineProvider;

  /**
   * Explicitly select a sandbox mode.
   * - `vm`: (Default on Node/Bun) Uses Node's `vm` module for a secure, native sandbox.
   * - `jsc`: Uses JavaScriptCore via JSI (Apple platforms only).
   * - `quickjs`: Uses QuickJS via JSI (cross-platform native).
   * - `wasm-quickjs`: Uses QuickJS via WASM (cross-platform, web-compatible).
   * - `none`: Runs code directly in the host context via `eval`. Insecure, but fast and easy to debug.
   * If not set, the best available provider for the environment is chosen automatically.
   */
  sandbox?: 'vm' | 'jsc' | 'quickjs' | 'wasm-quickjs' | 'none';

  /**
   * Execution timeout (milliseconds)
   * @default 5000
   */
  timeout?: number;

  /**
   * Enable debug mode
   * @default false
   */
  debug?: boolean;

  /**
   * Custom logger
   */
  logger?: {
    // Reason: Logger methods accept arbitrary console arguments
    log: (...args: unknown[]) => void;
    warn: (...args: unknown[]) => void;
    error: (...args: unknown[]) => void;
  };

  /**
   * Allowed modules for sandbox require()
   * If not provided, a safe default whitelist will be used
   */
  requireWhitelist?: readonly string[];

  /**
   * Performance metrics reporter hook
   * Called with metric name and duration in ms
   */
  onMetric?: (name: string, value: number, extra?: Record<string, unknown>) => void;

  /**
   * Maximum operations per batch applied by Receiver
   * Excess operations are skipped to protect host performance
   * @default 5000
   */
  receiverMaxBatchSize?: number;

  /**
   * Diagnostics parameters (for Host-side Task Manager/Resource Monitor)
   */
  diagnostics?: {
    /**
     * Stats window (ms) for calculating ops/s and batch/s
     * @default 5000
     */
    activityWindowMs?: number;
    /**
     * Activity sample retention duration (ms), for timeline aggregation
     * @default 60000
     */
    activityHistoryMs?: number;
    /**
     * Timeline bucket width (ms)
     * @default 2000
     */
    activityBucketMs?: number;
  };

  /**
   * DevTools configuration
   * - true: Enable with default settings
   * - false/undefined: Disable (default)
   * - RuntimeCollectorConfig: Enable with custom settings
   */
  devtools?: boolean | RuntimeCollectorConfig;
}

/**
 * Event listener type
 */
export type EventListener<T> = (data: T) => void;

/**
 * Error types for better classification
 */
export class RequireError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'RequireError';
  }
}

export class ExecutionError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'ExecutionError';
  }
}

export class TimeoutError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'TimeoutError';
  }
}

// Re-export sandbox provider types
export type {
  JSEngineContext,
  JSEngineProvider,
  JSEngineRuntime,
  JSEngineRuntimeOptions,
} from '../../sandbox';
// Re-export IEngine types
export type {
  EngineActivityStats,
  EngineActivityTimeline,
  EngineActivityTimelinePoint,
  EngineDiagnostics,
  EngineEvents,
  EngineHealth,
  GuestMessage,
  IEngine,
} from '../IEngine';
