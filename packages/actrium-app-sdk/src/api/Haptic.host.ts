/**
 * Haptic - Host Implementation
 *
 * Provides haptic feedback functionality.
 * Note: Requires react-native-haptic-feedback or similar library in host app.
 */

import type { HapticAPI, HapticType } from '../types';

/**
 * Extended interface for internal access by core modules
 */
export interface HostHapticInternal extends HapticAPI {
  /** Set custom trigger handler (for host app to inject haptic library) */
  _setHandler(handler: (type?: HapticType) => void): void;
  /** Clear custom handler */
  _clearHandler(): void;
}

class HostHaptic implements HostHapticInternal {
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
   * Set custom trigger handler
   * @internal
   */
  _setHandler(handler: (type?: HapticType) => void): void {
    this.customTriggerHandler = handler;
  }

  /**
   * Clear custom handler
   * @internal
   */
  _clearHandler(): void {
    this.customTriggerHandler = undefined;
  }
}

export const Haptic: HostHapticInternal = new HostHaptic();

// Export class for testing
export { HostHaptic };
