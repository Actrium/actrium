/**
 * AskIt Registry - Modules Only
 *
 * Only contains module registration needed by Bridge (toast/haptic...), avoiding Host UI components,
 * so that core/bridge can be safely imported in non-RN environments (bun test).
 */

import {
  HAPTIC_CLEAR_HANDLER,
  HAPTIC_SET_HANDLER,
  Haptic,
  type HostHaptic,
} from '../api/Haptic.host';
import { type HostToast, TOAST_CLEAR_HANDLER, TOAST_SET_HANDLER, Toast } from '../api/Toast.host';

/**
 * Module registry for Bridge routing
 * Maps module names to their host implementations
 */
export const modules = {
  toast: Toast,
  haptic: Haptic,
} as const;

/**
 * AskIt module permission constraints (Phase 2: optional reject/degrade)
 *
 * Convention: Guest declares permissions via `.actr/manifest.json -> permissions: string[]`;
 * Host can optionally do warn/deny at bridge layer.
 */
export const MODULE_PERMISSIONS: Record<string, string | undefined> = {
  toast: 'toast',
  haptic: 'haptic',
} as const;

/**
 * Configure Toast with custom handler (e.g., for iOS)
 */
export function configureToast(
  handler: (message: string, options?: Parameters<typeof Toast.show>[1]) => void
): void {
  (Toast as HostToast)[TOAST_SET_HANDLER](handler);
}

/**
 * Clear Toast custom handler
 */
export function clearToastHandler(): void {
  (Toast as HostToast)[TOAST_CLEAR_HANDLER]();
}

/**
 * Configure Haptic with custom handler
 */
export function configureHaptic(
  handler: (type?: Parameters<typeof Haptic.trigger>[0]) => void
): void {
  (Haptic as HostHaptic)[HAPTIC_SET_HANDLER](handler);
}

/**
 * Clear Haptic custom handler
 */
export function clearHapticHandler(): void {
  (Haptic as HostHaptic)[HAPTIC_CLEAR_HANDLER]();
}
