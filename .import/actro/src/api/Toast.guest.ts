/**
 * Toast - Guest Implementation
 *
 * In guest environment, Toast sends commands to Host through the bridge.
 */

import type { ToastAPI, ToastOptions } from '../types';
import { logger } from '../core/logger';

// Declare globals injected by Rill runtime
import type { TypedSendToHost } from '../types';

declare const global: {
  sendToHost?: TypedSendToHost;
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

    if (typeof global.sendToHost === 'function') {
      try {
        // Send as array to avoid serialization issues
        global.sendToHost('askit:toast:show', [message, options]);
      } catch (error) {
        logger.error('Toast', 'Failed to send message to host', {
          message,
          options,
          error: error instanceof Error ? error.message : String(error),
          hasSendToHost: typeof global.sendToHost === 'function',
        });
        // Fallback to console
        console.log(`[Toast] ${message}`);
      }
    } else {
      logger.warn('Toast', 'sendToHost not available in this environment');
      console.log(`[Toast] ${message}`);
    }
  }
}

export const Toast: ToastAPI = new RemoteToast();
