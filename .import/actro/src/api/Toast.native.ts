/**
 * Toast - Native Implementation (Host App)
 *
 * Provides toast notification functionality using React Native APIs.
 */

import type { ToastAPI, ToastOptions, ToastDuration, ToastPosition } from '../types';

// Types for React Native modules
type PlatformType = { OS: string };
type ToastAndroidType = {
  LONG: number;
  SHORT: number;
  TOP: number;
  CENTER: number;
  BOTTOM: number;
  showWithGravity: (message: string, duration: number, gravity: number) => void;
};

// Lazy load react-native to allow testing without RN environment
let Platform: PlatformType | null = null;
let ToastAndroid: ToastAndroidType | null = null;
let loadAttempted = false;

function loadReactNative(): boolean {
  if (loadAttempted) return Platform !== null;
  loadAttempted = true;
  try {
    const rn = require('react-native');
    Platform = rn.Platform;
    ToastAndroid = rn.ToastAndroid;
    return true;
  } catch {
    return false;
  }
}

/**
 * Inject mock modules for testing
 * @internal
 */
export function _injectMocks(platform: PlatformType, toastAndroid: ToastAndroidType): void {
  Platform = platform;
  ToastAndroid = toastAndroid;
  loadAttempted = true;
}

/**
 * Reset loaded modules (for testing)
 * @internal
 */
export function _resetReactNative(): void {
  Platform = null;
  ToastAndroid = null;
  loadAttempted = false;
}

/**
 * Convert duration to milliseconds
 */
export function getDurationMs(duration?: ToastDuration): number {
  if (typeof duration === 'number') {
    return duration;
  }
  switch (duration) {
    case 'long':
      return 3500;
    case 'short':
    default:
      return 2000;
  }
}

/**
 * Map position to gravity constant
 */
export function getGravityValue(position?: ToastPosition): 'top' | 'center' | 'bottom' {
  switch (position) {
    case 'top':
      return 'top';
    case 'center':
      return 'center';
    case 'bottom':
    default:
      return 'bottom';
  }
}

/**
 * Native Toast implementation
 * - Android: Uses ToastAndroid
 * - iOS: Falls back to console.log (requires custom implementation in host app)
 */
class NativeToast implements ToastAPI {
  private customShowHandler?: (message: string, options?: ToastOptions) => void;

  /**
   * Show a toast message
   */
  show(message: string, options?: ToastOptions): void {
    // If custom handler is set, use it (allows host app to override)
    if (this.customShowHandler) {
      this.customShowHandler(message, options);
      return;
    }

    // Try to load React Native
    if (loadReactNative() && Platform && ToastAndroid && Platform.OS === 'android') {
      const duration =
        getDurationMs(options?.duration) > 2500 ? ToastAndroid.LONG : ToastAndroid.SHORT;

      // Android Toast with gravity based on position
      const gravity = this.getGravity(options?.position);
      ToastAndroid.showWithGravity(message, duration, gravity);
    } else {
      // iOS or non-RN environment - log to console
      console.log(`[Toast] ${message}`);
    }
  }

  /**
   * Get Android gravity constant from position
   */
  private getGravity(position?: ToastPosition): number {
    if (!ToastAndroid) return 0;
    switch (position) {
      case 'top':
        return ToastAndroid.TOP;
      case 'center':
        return ToastAndroid.CENTER;
      case 'bottom':
      default:
        return ToastAndroid.BOTTOM;
    }
  }

  /**
   * Set custom show handler (for iOS or custom toast library)
   * @internal
   */
  _setCustomHandler(handler: (message: string, options?: ToastOptions) => void): void {
    this.customShowHandler = handler;
  }

  /**
   * Clear custom handler
   * @internal
   */
  _clearCustomHandler(): void {
    this.customShowHandler = undefined;
  }
}

export const Toast: ToastAPI = new NativeToast();

// Export class for core module to extend
export { NativeToast };
