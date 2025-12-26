/**
 * Sandbox Global Type Definitions
 *
 * Centralized type definitions for globals available in the sandbox environment.
 * These are injected by the Engine into the Guest context.
 */

/**
 * RillReconciler interface - the Guest-side reconciler API
 * Injected into sandbox as a global via GUEST_BUNDLE_CODE
 */
export interface RillReconcilerGlobal {
  render: (element: unknown, sendToHost: unknown) => unknown;
  unmount: (sendToHost: unknown) => void;
  unmountAll: () => void;
  invokeCallback?: (fnId: string, args: unknown[]) => unknown;
  releaseCallback?: (fnId: string) => void;
  registerComponentType?: (fn: unknown, engineId?: string) => string | null;
  unregisterComponentTypes?: (ownerId: string) => void;
  getCallbackCount?: () => number;
}

/**
 * Rill hooks state - used for useState/useEffect tracking
 */
export interface RillHooksState {
  index: number;
  rootElement?: unknown;
  sendToHost?: unknown;
}

/**
 * Type-safe accessor for sandbox globals
 * Use this instead of `as any` when accessing sandbox context
 */
export interface SandboxGlobals {
  RillReconciler?: RillReconcilerGlobal;
  __rillHooks?: RillHooksState;
  __REACT_SHIM__?: boolean;
  __invokeCallback?: (fnId: string, args: unknown[]) => unknown;
  __registerCallback?: (fn: (...args: unknown[]) => unknown) => string;
  __callbacks?: Map<string, (...args: unknown[]) => unknown>;
  __callbackId?: number;
  __RILL_GUEST_ENV__?: boolean;
  __RILL_DEBUG__?: boolean;
  __RILL_DEVTOOLS_ENABLED__?: boolean;
  __sendEventToHost?: (eventName: string, payload?: unknown) => void;
  __handleHostEvent?: (eventName: string, payload: unknown) => void;
  __useHostEvent?: unknown;
  __getConfig?: unknown;
  React?: unknown;
  ReactJSXRuntime?: unknown;
}

/**
 * Helper to safely get a sandbox global with proper typing
 */
export function getSandboxGlobal<K extends keyof SandboxGlobals>(
  context: { getGlobal: (name: string) => unknown } | null | undefined,
  key: K
): SandboxGlobals[K] {
  return context?.getGlobal(key) as SandboxGlobals[K];
}
