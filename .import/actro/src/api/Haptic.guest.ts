/**
 * Haptic - Guest Implementation
 *
 * In guest environment, Haptic sends commands to Host through the bridge.
 */

import { logger } from '../core/logger';
// Declare globals injected by Rill runtime
import type { HapticAPI, HapticType, TypedSendToHost } from '../types';

declare const global: {
  sendToHost?: TypedSendToHost;
};

class RemoteHaptic implements HapticAPI {
  /**
   * Trigger haptic feedback (sends command to Host)
   */
  trigger(type: HapticType = 'medium'): void {
    if (typeof global.sendToHost === 'function') {
      try {
        // Send as array for consistency with Toast
        global.sendToHost('askit:haptic:trigger', [type]);
      } catch (error) {
        logger.error('Haptic', 'Failed to send message to host', {
          type,
          error: error instanceof Error ? error.message : String(error),
          hasSendToHost: typeof global.sendToHost === 'function',
        });
      }
    } else {
      logger.warn('Haptic', 'sendToHost not available in this environment');
    }
  }
}

export const Haptic: HapticAPI = new RemoteHaptic();
