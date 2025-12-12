/**
 * AskIt Registry - Component and Module Registration
 *
 * This module provides the registry that connects askit components
 * to the Rill engine. Host apps import this to inject native implementations.
 */

import {
  HAPTIC_CLEAR_HANDLER,
  HAPTIC_SET_HANDLER,
  Haptic,
  type HostHaptic,
} from '../api/Haptic.host';
// Import host module implementations
import { type HostToast, TOAST_CLEAR_HANDLER, TOAST_SET_HANDLER, Toast } from '../api/Toast.host';
import { ChatBubble } from '../ui/ChatBubble/ChatBubble.host';
// Import host component implementations
import { StepList } from '../ui/StepList/StepList.host';
import { ThemeView } from '../ui/ThemeView/ThemeView.host';
import { UserAvatar } from '../ui/UserAvatar/UserAvatar.host';

/**
 * Component registry for Rill engine
 *
 * Usage:
 * ```typescript
 * import { Engine } from 'rill';
 * import { components } from 'askit/core';
 *
 * const engine = new Engine();
 * engine.register(components);
 * ```
 */
export const components = {
  StepList,
  ThemeView,
  UserAvatar,
  ChatBubble,
} as const;

/**
 * Module registry for Bridge routing
 * Maps module names to their host implementations
 */
export const modules = {
  toast: Toast,
  haptic: Haptic,
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
