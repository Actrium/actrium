/**
 * Haptic - Native Implementation (Host App)
 *
 * Provides haptic feedback functionality.
 * Note: Requires react-native-haptic-feedback or similar library in host app.
 */

import type { HapticAPI, HapticType } from '../types';

class NativeHaptic implements HapticAPI {
  private customTriggerHandler?: (type?: HapticType) => void;

  /**
   * Trigger haptic feedback
   */
  trigger(type: HapticType = 'medium'): void {
    if (this.customTriggerHandler) {
      this.customTriggerHandler(type);
      return;
    }

    // Default: log to console, host app should provide custom implementation
    console.log(`[Haptic] ${type}`);
  }

  /**
   * Set custom trigger handler (for host app to inject haptic library)
   * @internal
   */
  _setCustomHandler(handler: (type?: HapticType) => void): void {
    this.customTriggerHandler = handler;
  }

  /**
   * Clear custom handler
   * @internal
   */
  _clearCustomHandler(): void {
    this.customTriggerHandler = undefined;
  }
}

export const Haptic: HapticAPI = new NativeHaptic();

// Export class for core module
export { NativeHaptic };
