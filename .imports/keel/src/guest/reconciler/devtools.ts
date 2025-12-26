/**
 * DevTools and Debug Utilities for Reconciler
 *
 * Provides debugging hooks and DevTools integration for Guest reconciler.
 */

// ============================================
// Global Debug Declarations
// ============================================

declare global {
  // Operation tracking (used by OperationCollector)
  // eslint-disable-next-line no-var
  var __OP_COUNTS: Record<string, number> | undefined;
  // eslint-disable-next-line no-var
  var __TOTAL_OPS: number | undefined;

  // Host config tracking (used by host-config.ts)
  // eslint-disable-next-line no-var
  var __APPEND_INITIAL_CALLED: number | undefined;
  // eslint-disable-next-line no-var
  var __APPEND_CHILD_CALLED: number | undefined;
  // eslint-disable-next-line no-var
  var __APPEND_TO_CONTAINER_CALLED: number | undefined;

  // Callback registry (used by host-config.ts)
  // eslint-disable-next-line no-var
  var __callbacks: Map<string, (...args: unknown[]) => unknown> | undefined;

  // DevTools flags
  // eslint-disable-next-line no-var
  var __RILL_DEVTOOLS_ENABLED: boolean | undefined;
  // eslint-disable-next-line no-var
  var __sendEventToHost: ((eventName: string, payload?: unknown) => void) | undefined;
}

// ============================================
// DevTools Types
// ============================================

export interface RenderTiming {
  nodeId: number;
  type: string;
  phase: 'mount' | 'update';
  duration: number;
  timestamp: number;
}

// ============================================
// DevTools Helpers
// ============================================

export function isDevToolsEnabled(): boolean {
  return globalThis.__RILL_DEVTOOLS_ENABLED === true;
}

export function sendDevToolsMessage(type: string, data: unknown): void {
  if (typeof globalThis.__sendEventToHost === 'function') {
    globalThis.__sendEventToHost(type, data);
  }
}
