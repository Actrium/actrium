/**
 * IEngine - Common interface for Engine implementations
 *
 * Each Engine instance owns a dedicated JS runtime/thread.
 * Create a new Engine for each isolated execution context needed.
 */

import type { OperationBatch, SerializedValue } from '../types';
import type { Receiver } from './receiver';
import type { ComponentMap, ComponentRegistry } from './registry';

/**
 * Message from guest to host
 */
export interface GuestMessage {
  event: string;
  payload: unknown;
}

/**
 * Engine event types
 */
export interface EngineEvents {
  load: () => void;
  error: (error: Error) => void;
  destroy: () => void;
  operation: (batch: OperationBatch) => void;
  message: (message: GuestMessage) => void;
  /**
   * Fatal error event - emitted when the engine encounters an unrecoverable error
   * such as execution timeout. The engine will be automatically destroyed after this event.
   */
  fatalError: (error: Error) => void;
}

/**
 * Health snapshot for observability
 */
export interface EngineHealth {
  loaded: boolean;
  destroyed: boolean;
  errorCount: number;
  lastErrorAt: number | null;
  receiverNodes: number;
  batching: boolean;
}

/**
 * Resource statistics for monitoring
 */
export interface ResourceStats {
  timers: number;
  nodes: number;
  callbacks: number;
}

/**
 * Common engine interface
 */
export interface IEngine {
  /**
   * Unique engine identifier
   */
  readonly id: string;

  /**
   * Register custom components
   */
  register(components: ComponentMap): void;

  /**
   * Load and execute Guest code
   */
  loadBundle(source: string, initialProps?: Record<string, unknown>): Promise<void>;

  /**
   * Subscribe to engine events
   * @returns Unsubscribe function
   */
  on<K extends keyof EngineEvents>(
    event: K,
    listener: EngineEvents[K] extends () => void
      ? () => void
      : (data: Parameters<EngineEvents[K]>[0]) => void
  ): () => void;

  /**
   * Send event to sandbox guest
   */
  sendEvent(eventName: string, payload?: unknown): void;

  /**
   * Update configuration
   */
  updateConfig(config: Record<string, SerializedValue>): void;

  /**
   * Create Receiver for rendering
   */
  createReceiver(onUpdate: () => void): Receiver;

  /**
   * Get current Receiver
   */
  getReceiver(): Receiver | null;

  /**
   * Get component registry
   */
  getRegistry(): ComponentRegistry;

  /**
   * Check if bundle is loaded
   */
  readonly isLoaded: boolean;

  /**
   * Check if engine is destroyed
   */
  readonly isDestroyed: boolean;

  /**
   * Get health snapshot for observability
   */
  getHealth(): EngineHealth;

  /**
   * Get resource statistics for monitoring
   */
  getResourceStats(): ResourceStats;

  /**
   * Set maximum number of listeners per event before warning
   * @param n - Maximum listener count (default: 10)
   */
  setMaxListeners(n: number): void;

  /**
   * Get current maximum listener threshold
   */
  getMaxListeners(): number;

  /**
   * Destroy engine and release resources
   */
  destroy(): void;
}
