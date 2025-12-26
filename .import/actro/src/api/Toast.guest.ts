/**
 * Toast - Guest Implementation
 *
 * In guest environment, Toast sends commands to Host through the bridge.
 */

import { logger } from '../core/logger';
import type { ToastAPI, ToastOptions } from '../types';

// Declare globals injected by Rill runtime
declare const global: {
  __sendEventToHost?: (eventName: string, payload?: unknown) => void;
};

class RemoteToast implements ToastAPI {
  /**
   * Show a toast message (sends command to Host)
   */
  show(message: string, options?: ToastOptions): void {
    // Validate message parameter
    if (!message || typeof message !== 'string') {
      logger.error('Toast', 'Invalid message parameter - must be a non-empty string');
      return;
    }

    if (typeof global.__sendEventToHost === 'function') {
      try {
        global.__sendEventToHost('ASKIT_TOAST_SHOW', { message, options });
      } catch (error) {
        logger.error('Toast', 'Failed to send message to host', {
          message,
          options,
          error: error instanceof Error ? error.message : String(error),
          hasSendToHost: typeof global.__sendEventToHost === 'function',
        });
        // Fallback to console
        console.log(`[Toast] ${message}`);
      }
    } else {
      logger.warn('Toast', '__sendEventToHost not available in this environment');
      console.log(`[Toast] ${message}`);
    }
  }
}

export const Toast: ToastAPI = new RemoteToast();
