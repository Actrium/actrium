/**
 * AskIt Registry - Modules Only
 *
 * Only contains module registration needed by Bridge (toast/haptic...), avoiding Host UI components,
 * so that core/bridge can be safely imported in non-RN environments (bun test).
 */

import { Haptic, type HostHapticInternal } from '../api/Haptic.host';
import { Toast, type HostToastInternal } from '../api/Toast.host';

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
  (Toast as HostToastInternal)._setHandler(handler);
}

/**
 * Clear Toast custom handler
 */
export function clearToastHandler(): void {
  (Toast as HostToastInternal)._clearHandler();
}

/**
 * Configure Haptic with custom handler
 */
export function configureHaptic(
  handler: (type?: Parameters<typeof Haptic.trigger>[0]) => void
): void {
  (Haptic as HostHapticInternal)._setHandler(handler);
}

/**
 * Clear Haptic custom handler
 */
export function clearHapticHandler(): void {
  (Haptic as HostHapticInternal)._clearHandler();
}
