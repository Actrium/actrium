/**
 * Haptic - Guest Implementation
 *
 * In guest environment, Haptic sends commands to Host through the bridge.
 */

import { logger } from '../core/logger';
import type { HapticAPI, HapticType } from '../types';

// Declare globals injected by Rill runtime
declare const global: {
  __sendEventToHost?: (eventName: string, payload?: unknown) => void;
};

class RemoteHaptic implements HapticAPI {
  /**
   * Trigger haptic feedback (sends command to Host)
   */
  trigger(type: HapticType = 'medium'): void {
    if (typeof global.__sendEventToHost === 'function') {
      try {
        global.__sendEventToHost('ASKIT_HAPTIC_TRIGGER', { type });
      } catch (error) {
        logger.error('Haptic', 'Failed to send message to host', {
          type,
          error: error instanceof Error ? error.message : String(error),
          hasSendToHost: typeof global.__sendEventToHost === 'function',
        });
      }
    } else {
      logger.warn('Haptic', '__sendEventToHost not available in this environment');
    }
  }
}

export const Haptic: HapticAPI = new RemoteHaptic();
