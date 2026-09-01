/**
 * EventEmitter - Host Implementation
 *
 * Provides event-based communication between Host and Guest engines.
 * In host environment, EventEmitter manages multiple engine instances and broadcasts events.
 */

import { logger } from '../core/logger';
import type { EventEmitterAPI } from '../types';
import { EventEmitterBase } from './EventEmitterBase';

/**
 * Event broadcast callback type
 * Used by createEngineAdapter to forward events to Rill Engine
 */
export type EventBroadcaster = (event: string, payload: unknown) => void;

/**
 * Extended interface for internal access by core modules
 */
export interface HostEventEmitterInternal extends EventEmitterAPI {
  /** Connect to engine broadcaster (called by createEngineAdapter) */
  _setBroadcaster(broadcaster: EventBroadcaster | null): void;
  /** Notify listeners without broadcasting to engine (avoids infinite loops) */
  _notifyLocal(event: string, payload: unknown): void;
}

class HostEventEmitter extends EventEmitterBase implements HostEventEmitterInternal {
  private broadcaster: EventBroadcaster | null = null;

  /**
   * Emit an event to all listeners and broadcast to engine if connected
   */
  emit(event: string, payload?: unknown): void {
    // Notify local listeners
    this.notifyLocalListeners(event, payload);

    // Broadcast to engine if connected
    if (this.broadcaster) {
      try {
        this.broadcaster(event, payload);
      } catch (error) {
        logger.error('EventEmitter', 'Error broadcasting to engine', {
          event,
          error: error instanceof Error ? error.message : String(error),
        });
      }
    }
  }

  /**
   * Connect to engine broadcaster (called by createEngineAdapter)
   * @internal
   */
  _setBroadcaster(broadcaster: EventBroadcaster | null): void {
    this.broadcaster = broadcaster;
  }

  /**
   * Notify listeners without broadcasting to engine
   * Used when receiving events from engine to avoid infinite loops
   * @internal
   */
  _notifyLocal(event: string, payload: unknown): void {
    this.notifyLocalListeners(event, payload);
  }
}

// Export singleton instance
export const EventEmitter: HostEventEmitterInternal = new HostEventEmitter();

// Export class for testing
export { HostEventEmitter };
