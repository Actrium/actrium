/**
 * Haptic - Guest Implementation
 *
 * In guest environment, Haptic sends commands to Host through the bridge.
 */

import { logger } from '../core/logger';
import type { HapticAPI, HapticType } from '../types';

// Globals injected into every Keel sandbox.
const runtimeGlobal = globalThis as typeof globalThis & {
  __keel_emitEvent?: (eventName: string, payload?: unknown) => void;
  __sendEventToHost?: (eventName: string, payload?: unknown) => void;
};

const getSendToHost = () =>
  runtimeGlobal.__keel_emitEvent ?? runtimeGlobal.__sendEventToHost;

class RemoteHaptic implements HapticAPI {
  /**
   * Trigger haptic feedback (sends command to Host)
   */
  trigger(type: HapticType = 'medium'): void {
    const sendToHost = getSendToHost();
    if (sendToHost) {
      try {
        sendToHost('ASKIT_HAPTIC_TRIGGER', { type });
      } catch (error) {
        logger.error('Haptic', 'Failed to send message to host', {
          type,
          error: error instanceof Error ? error.message : String(error),
          hasSendToHost: true,
        });
      }
    } else {
      logger.warn('Haptic', '__sendEventToHost not available in this environment');
    }
  }
}

export const Haptic: HapticAPI = new RemoteHaptic();
