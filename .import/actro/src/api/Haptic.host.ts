/**
 * Haptic - Host Implementation
 *
 * Provides haptic feedback functionality.
 * Note: Requires react-native-haptic-feedback or similar library in host app.
 */

import type { HapticAPI, HapticType } from '../types';

/**
 * Internal symbols for accessing private APIs
 */
const SET_HANDLER_SYMBOL = Symbol.for('askit.haptic.setHandler');
const CLEAR_HANDLER_SYMBOL = Symbol.for('askit.haptic.clearHandler');

class HostHaptic implements HapticAPI {
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
   * @internal - Accessed via SET_HANDLER_SYMBOL
   */
  [SET_HANDLER_SYMBOL](handler: (type?: HapticType) => void): void {
    this.customTriggerHandler = handler;
  }

  /**
   * Clear custom handler
   * @internal - Accessed via CLEAR_HANDLER_SYMBOL
   */
  [CLEAR_HANDLER_SYMBOL](): void {
    this.customTriggerHandler = undefined;
  }
}

export const Haptic: HapticAPI = new HostHaptic();

// Export class for core module
export { HostHaptic };

// Export symbols for core module access
export { SET_HANDLER_SYMBOL as HAPTIC_SET_HANDLER, CLEAR_HANDLER_SYMBOL as HAPTIC_CLEAR_HANDLER };
