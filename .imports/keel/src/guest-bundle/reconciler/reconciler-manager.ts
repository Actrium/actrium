/**
 * Reconciler Manager
 *
 * Manages reconciler instances and provides public API for rendering.
 * Each sendToHost function gets its own isolated reconciler instance.
 */

import type { ReactElement } from 'react';
import { type CallbackRegistry, CallbackRegistryImpl, type SendToHost } from '../../bridge';
import type { GuestElement } from '../../let/types';
import { transformGuestElement } from './element-transform';
import { createReconciler } from './host-config';
import type { RillReconciler, RootContainer } from './types';

// ============================================
// Reconciler Instance Management
// ============================================

/**
 * Reconciler instance data for each guest
 */
interface ReconcilerInstance {
  reconciler: RillReconciler;
  callbackRegistry: CallbackRegistry;
  collector: ReturnType<typeof createReconciler>['collector'];
  container: RootContainer;
  root: ReturnType<RillReconciler['createContainer']>;
}

/**
 * Map of sendToHost functions to their reconciler instances
 * This allows multiple guests to each have their own isolated reconciler
 */
const reconcilerMap = new Map<SendToHost, ReconcilerInstance>();

/**
 * Global callback registry for Guest environment (sandbox internal use)
 *
 * Architecture notes:
 * - In Host: Each Engine instance owns its own CallbackRegistry (single ownership)
 * - In Guest: This globalCallbackRegistry is used for sandbox-internal operations
 *
 * The Host-side Engine.callbackRegistry is injected into Bridge and handles
 * callbacks for props passed across the JSI boundary. This globalCallbackRegistry is for:
 * - Guest function component transformations (serializePropsForGuest)
 * - Legacy compatibility within sandbox context
 *
 * For new code, prefer using Engine's callbackRegistry via Bridge.
 */
export const globalCallbackRegistry = new CallbackRegistryImpl();

// ============================================
// Public API
// ============================================

/**
 * Render React element
 *
 * Each sendToHost function creates and manages its own isolated reconciler instance.
 * This allows multiple guests to run simultaneously without interfering with each other.
 *
 * Note: Bridge now handles all serialization internally.
 * The sendToHost should be Bridge.sendToHost for proper serialization.
 *
 * @param element - React element to render
 * @param sendToHost - Function to send operations to host (typically Bridge.sendToHost)
 */
export function render(element: ReactElement, sendToHost: SendToHost): void {
  const debugEnabled = Boolean((globalThis as Record<string, unknown>).__RILL_RECONCILER_DEBUG__);
  if (debugEnabled) {
    const elAny = element as unknown as Record<string, unknown>;
    try {
      const props = elAny?.props as Record<string, unknown> | undefined;
      console.log(
        `[rill:reconciler] render called | type=${String(elAny?.type)} | typeType=${typeof elAny?.type} | marker=${String(elAny?.__rillTypeMarker)} | fragment=${String(elAny?.__rillFragmentType)} | propsKeys=${props ? Object.keys(props).join(',') : 'null'}`
      );
    } catch {
      console.log('[rill:reconciler] render called (log format failed)');
    }
  }

  // Transform Guest element to use Host's Symbol registry
  // ReactElement is compatible with GuestElement at runtime
  const transformedElement = transformGuestElement(element as unknown as GuestElement);
  if (debugEnabled) {
    try {
      const t = transformedElement as ReactElement | null;
      console.log(
        `[rill:reconciler] transformed element | type=${String(t?.type)} | typeType=${typeof t?.type} | propsKeys=${t?.props ? Object.keys(t.props as Record<string, unknown>).join(',') : 'null'}`
      );
    } catch {
      console.log('[rill:reconciler] transformed element (log format failed)');
    }
  }

  let instance = reconcilerMap.get(sendToHost);

  if (!instance) {
    // Create new reconciler instance for this guest
    // Note: sendToHost should be Bridge.sendToHost for proper serialization
    const reconcilerInstance = createReconciler(sendToHost, globalCallbackRegistry);
    const container: RootContainer = { children: [] };
    const root = reconcilerInstance.reconciler.createContainer(
      container,
      0, // ConcurrentRoot
      null, // hydrationCallbacks
      false, // isStrictMode
      null, // concurrentUpdatesByDefaultOverride
      'rill', // identifierPrefix
      (error: Error) => console.error('[rill] onUncaughtError:', error), // onUncaughtError
      (error: Error) => console.error('[rill] onCaughtError:', error), // onCaughtError
      (error: Error) => console.error('[rill] onRecoverableError:', error), // onRecoverableError
      () => {}, // onDefaultTransitionIndicator
      null // transitionCallbacks
    );

    instance = {
      ...reconcilerInstance,
      container,
      root,
    };

    reconcilerMap.set(sendToHost, instance);
  }

  instance.reconciler.updateContainer(transformedElement, instance.root, null, () => {});
}

/**
 * Unmount a specific guest instance
 */
export function unmount(sendToHost?: SendToHost): void {
  if (!sendToHost) return;
  const instance = reconcilerMap.get(sendToHost);
  if (instance) {
    instance.reconciler.updateContainer(null, instance.root, null, () => {});
    // DO NOT clear the global callback registry here, as it's shared
    // instance.callbackRegistry.clear();
    reconcilerMap.delete(sendToHost);
  }
}

/**
 * Unmount all guest instances
 */
export function unmountAll(): void {
  reconcilerMap.forEach((instance) => {
    instance.reconciler.updateContainer(null, instance.root, null, () => {});
    instance.callbackRegistry.clear();
  });
  reconcilerMap.clear();
}

/**
 * Get callback registry for a specific guest instance
 * Used for resource monitoring and debugging
 */
export function getCallbackRegistry(sendToHost?: SendToHost): CallbackRegistry | null {
  if (!sendToHost) {
    return null;
  }
  const instance = reconcilerMap.get(sendToHost);
  return instance ? instance.callbackRegistry : null;
}

/**
 * Check if a callback exists (global registry)
 *
 * Note: In RN / VM runtimes, Guest-side functions are seen by Host-side reconciler
 * as callable handles and registered in CallbackRegistry.
 * No need to rely on Guest runtime injected globalThis.__callbacks / __invokeCallback.
 */
export function hasCallback(fnId: string): boolean {
  return globalCallbackRegistry.getMap().has(fnId);
}

/**
 * Invoke a callback (global registry)
 */
export function invokeCallback(fnId: string, args: unknown[] = []): unknown {
  return globalCallbackRegistry.invoke(fnId, args);
}

/**
 * Release a callback (global registry)
 * Used by Host's Bridge for cleanup
 */
export function releaseCallback(fnId: string): void {
  globalCallbackRegistry.release(fnId);
}

/**
 * Get callback count (global registry)
 * Used by Host for testing/debugging
 */
export function getCallbackCount(): number {
  return globalCallbackRegistry.size;
}
