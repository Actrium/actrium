/**
 * AskIt Registry - Modules Only
 *
 * 仅包含 Bridge 需要的模块注册（toast/haptic...），避免引入 Host UI 组件，
 * 使得 core/bridge 可以在非 RN 环境（bun test）下被安全导入。
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
 * AskIt 模块权限约束（Phase 2：可选拒绝/降级）
 *
 * 约定：Guest 通过 `.actr/manifest.json -> permissions: string[]` 声明权限；
 * Host 可选择在 bridge 层做 warn/deny。
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
