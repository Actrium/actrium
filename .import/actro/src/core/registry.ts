/**
 * AskIt Registry - Component and Module Registration
 *
 * This module provides the registry that connects askit components
 * to the Rill engine. Host apps import this to inject native implementations.
 */

import type { ComponentMap, ModuleMap, AskitRegistryConfig } from '../types';

// Import native component implementations
import { StepList } from '../ui/StepList/StepList.native';
import { ThemeView } from '../ui/ThemeView/ThemeView.native';
import { UserAvatar } from '../ui/UserAvatar/UserAvatar.native';
import { ChatBubble } from '../ui/ChatBubble/ChatBubble.native';

// Import native module implementations
import { Toast as ToastModule, NativeToast } from '../api/Toast.native';
import { Haptic as HapticModule, NativeHaptic } from '../api/Haptic.native';

/**
 * Default component registry
 * Maps component names to their native React implementations
 */
export const AskitComponents: ComponentMap = {
  StepList,
  ThemeView,
  UserAvatar,
  ChatBubble,
};

/**
 * Module handlers for plugin API calls
 * Processes messages like `askit:toast:show` from plugins
 */
export interface ModuleHandler {
  handle: (method: string, args: unknown[]) => unknown;
}

/**
 * Toast module handler
 */
const ToastHandler: ModuleHandler = {
  handle(method: string, args: unknown[]) {
    if (method === 'show') {
      const [message, options] = args as [string, unknown];
      ToastModule.show(message, options as Parameters<typeof ToastModule.show>[1]);
    }
  },
};

/**
 * Haptic module handler
 */
const HapticHandler: ModuleHandler = {
  handle(method: string, args: unknown[]) {
    if (method === 'trigger') {
      const [type] = args as [Parameters<typeof HapticModule.trigger>[0]];
      HapticModule.trigger(type);
    }
  },
};

/**
 * Default module registry
 */
export const AskitModules: Record<string, ModuleHandler> = {
  toast: ToastHandler,
  haptic: HapticHandler,
};

/**
 * Full registry configuration for Rill engine
 */
export const AskitRegistry: AskitRegistryConfig = {
  components: AskitComponents,
  modules: AskitModules as unknown as ModuleMap,
};

/**
 * Configure Toast with custom handler (e.g., for iOS)
 */
export function configureToast(
  handler: (message: string, options?: Parameters<typeof ToastModule.show>[1]) => void
): void {
  (ToastModule as NativeToast)._setCustomHandler(handler);
}

/**
 * Configure Haptic with custom handler
 */
export function configureHaptic(
  handler: (type?: Parameters<typeof HapticModule.trigger>[0]) => void
): void {
  (HapticModule as NativeHaptic)._setCustomHandler(handler);
}

export { AskitComponents as components, AskitModules as modules };
