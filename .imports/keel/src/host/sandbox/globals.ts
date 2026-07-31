/**
 * Sandbox Global Type Definitions
 *
 * Centralized type definitions for globals available in the sandbox environment.
 * These are injected by the Engine into the Guest context.
 */

import type { HostMessage, ReviewedUnknown, SendToHost } from '../types';

/**
 * KeelReconciler interface - the Guest-side reconciler API
 * Injected into sandbox as a global via GUEST_BUNDLE_CODE
 */
export interface KeelReconcilerGlobal {
  render: (element: ReviewedUnknown, sendToHost: SendToHost) => void;
  unmount: (sendToHost?: SendToHost) => void;
  unmountAll: () => void;
  invokeCallback?: (fnId: string, args: ReviewedUnknown[]) => ReviewedUnknown;
  releaseCallback?: (fnId: string) => void;
  registerComponentType?: (fn: ReviewedUnknown, engineId?: string) => string | null;
  unregisterComponentTypes?: (ownerId: string) => void;
  getCallbackCount?: () => number;
}

/**
 * Keel hooks state - used for useState/useEffect tracking
 */
export interface KeelHooksState {
  index: number;
  rootElement?: ReviewedUnknown;
  sendToHost?: SendToHost;
}

export type KeelEventCallback = (payload: ReviewedUnknown) => void;

export type KeelOnHostEvent = (eventName: string, callback: KeelEventCallback) => () => void;

/**
 * Type-safe accessor for sandbox globals
 * Use this instead of `as any` when accessing sandbox context
 */
export interface SandboxGlobals {
  KeelReconciler?: KeelReconcilerGlobal;
  __keelHooks?: KeelHooksState;
  __REACT_SHIM__?: boolean;
  __keel?: {
    callbacks?: Map<string, (...args: ReviewedUnknown[]) => ReviewedUnknown>;
    callbackId?: number;
    registerCallback?: (fn: (...args: ReviewedUnknown[]) => ReviewedUnknown) => string;
    invokeCallback?: (fnId: string, args: ReviewedUnknown[]) => ReviewedUnknown;
    removeCallback?: (id: string) => void;
    config?: Record<string, ReviewedUnknown>;
    eventListeners?: Map<string, Set<KeelEventCallback>>;
    dispatchEvent?: (eventName: string, payload: ReviewedUnknown) => void;
    guest?: ReviewedUnknown;
  };
  __keel_emitEvent?: (eventName: string, payload?: ReviewedUnknown) => void;
  __keel_onHostEvent?: KeelOnHostEvent;
  __keel_getConfig?: () => Record<string, ReviewedUnknown>;
  __keel_sendBatch?: SendToHost;
  __keel_sendOperation?: (op: ReviewedUnknown) => void;
  __keel_handleMessage?: (message: HostMessage) => void;
  __keel_scheduleRender?: () => void;
  __keel_registerComponentType?: (fn: ReviewedUnknown) => string | null;
  __KEEL_GUEST_ENV__?: boolean;
  __KEEL_DEBUG__?: boolean;
  __KEEL_DEVTOOLS_ENABLED__?: boolean;
  React?: ReviewedUnknown;
  ReactJSXRuntime?: ReviewedUnknown;
}

/**
 * Helper to safely get a sandbox global with proper typing
 */
export function getSandboxGlobal<K extends keyof SandboxGlobals>(
  context: { extract: (name: string) => ReviewedUnknown } | null | undefined,
  key: K
): SandboxGlobals[K] {
  return context?.extract(key) as SandboxGlobals[K];
}
