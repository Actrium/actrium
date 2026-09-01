/**
 * Fast Path Forwarder -
 *
 * Forwards WebRTC DataChannel messages to the Service Worker WASM layer.
 */

import { ServiceWorkerBridge } from './sw-bridge';

export interface FastPathData {
  streamId: string;
  data: ArrayBuffer;
  timestamp: number;
}

/**
 * Fast Path forwarder.
 */
export class FastPathForwarder {
  private swBridge: ServiceWorkerBridge;
  private batchQueue: FastPathData[] = [];
  private batchTimer: number | null = null;
  private batchSize = 10; // max items per batch
  private batchTimeoutMs = 5; // flush timeout in ms

  constructor(swBridge: ServiceWorkerBridge) {
    this.swBridge = swBridge;
  }

  /**
   * Forward data immediately via the Fast Path to the Service Worker.
   *
   * Uses Transferable ArrayBuffer for zero-copy transfer.
   */
  forward(streamId: string, data: ArrayBuffer): void {
    const fastPathData: FastPathData = {
      streamId,
      data,
      timestamp: Date.now(),
    };

    // Forward immediately (no batching)
    this.forwardImmediate(fastPathData);
  }

  /**
   * Forward data in batches via the Fast Path.
   *
   * Buffers messages and flushes via PostMessage when batch is full or timer fires.
   */
  forwardBatch(streamId: string, data: ArrayBuffer): void {
    this.batchQueue.push({
      streamId,
      data,
      timestamp: Date.now(),
    });

    // If batch is full, flush immediately
    if (this.batchQueue.length >= this.batchSize) {
      this.flushBatch();
    } else if (this.batchTimer === null) {
      // Otherwise, start the flush timer
      this.batchTimer = window.setTimeout(() => {
        this.flushBatch();
      }, this.batchTimeoutMs);
    }
  }

  /**
   * Forward a single message immediately.
   */
  private forwardImmediate(fastPathData: FastPathData): void {
    const view = new Uint8Array(fastPathData.data);
    this.swBridge.sendToSW(
      {
        type: 'fast_path_data',
        payload: {
          streamId: fastPathData.streamId,
          data: view,
          timestamp: fastPathData.timestamp,
        },
      },
      [view.buffer as ArrayBuffer] // Transferable -
    );
  }

  /**
   * Forward a single message immediately.
   */
  private flushBatch(): void {
    if (this.batchQueue.length === 0) {
      return;
    }

    //
    if (this.batchTimer !== null) {
      window.clearTimeout(this.batchTimer);
      this.batchTimer = null;
    }

    //  transferables
    const batchPayload = this.batchQueue.map((item) => ({
      streamId: item.streamId,
      data: new Uint8Array(item.data),
      timestamp: item.timestamp,
    }));
    const transferables: ArrayBuffer[] = batchPayload.map(
      (item) => item.data.buffer as ArrayBuffer
    );

    //
    this.swBridge.sendToSW(
      {
        type: 'fast_path_data',
        payload: {
          batch: batchPayload,
        },
      },
      transferables
    );

    //
    this.batchQueue = [];
  }

  /**
   * Forward a single message immediately.
   */
  setBatchParams(size: number, timeoutMs: number): void {
    this.batchSize = size;
    this.batchTimeoutMs = timeoutMs;
  }

  /**
   * Forward a single message immediately.
   */
  getMetrics(): {
    queueLength: number;
    hasPendingBatch: boolean;
  } {
    return {
      queueLength: this.batchQueue.length,
      hasPendingBatch: this.batchTimer !== null,
    };
  }

  /**
   * Forward a single message immediately.
   */
  dispose(): void {
    if (this.batchTimer !== null) {
      window.clearTimeout(this.batchTimer);
      this.batchTimer = null;
    }
    this.batchQueue = [];
  }
}
