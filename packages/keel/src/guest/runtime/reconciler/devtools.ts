/**
 * DevTools and Debug Utilities for Reconciler
 *
 * Provides debugging hooks and DevTools integration for Guest reconciler.
 */

// ============================================
// Global Debug Declarations
// ============================================

declare global {
  // DevTools flags
  // eslint-disable-next-line no-var
  var __KEEL_DEVTOOLS_ENABLED: boolean | undefined;
  // eslint-disable-next-line no-var
  // Reason: DevTools payload can be any serializable type
  var __keel_emitEvent: ((eventName: string, payload?: unknown) => void) | undefined;
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
  return globalThis.__KEEL_DEVTOOLS_ENABLED === true;
}

export function sendDevToolsMessage(type: string, data: unknown): void {
  if (typeof globalThis.__keel_emitEvent === 'function') {
    globalThis.__keel_emitEvent(type, data);
  }
}
