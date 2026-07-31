/**
 * Operation Collector
 * Collects operations during render phase, sends all during commit phase
 */

import type { SendToHost, SerializedOperation, SerializedOperationBatch } from '../../../sdk/types';

// Max entries kept in the globalThis.__keel_drain_diag debug buffer (oldest dropped first).
const DIAG_BUFFER_LIMIT = 200;

export class OperationCollector {
  private operations: SerializedOperation[] = [];
  private batchId = 0;
  private version = 1;
  private _flushing = false;

  private isDebugEnabled(): boolean {
    try {
      return Boolean((globalThis as Record<string, unknown>).__KEEL_RECONCILER_DEBUG__);
    } catch {
      return false;
    }
  }

  /**
   * Add operation
   */
  add(op: SerializedOperation): void {
    this.operations.push({
      ...op,
      timestamp: Date.now(),
    });

    // Debug aid: a little output, enough to confirm CREATE/APPEND/UPDATE are being collected (off by default)
    if (this.isDebugEnabled()) {
      const len = this.operations.length;
      if (len <= 10 || len % 50 === 0) {
        console.log('[keel:reconciler] add op', op.op, 'len', len);
      }
    }
  }

  /**
   * Flush and send all operations
   */
  flush(sendToHost: SendToHost): void {
    // Reentry guard: prevent flush-during-flush if sendToHost synchronously
    // triggers state changes that cause another commit/flush cycle.
    if (this._flushing) {
      return;
    }
    this._flushing = true;

    try {
      // Diagnostic accumulator: write to globalThis.__keel_drain_diag for HOST readout.
      // Debug-only; buffer is capped at DIAG_BUFFER_LIMIT entries (oldest dropped first).
      if (this.isDebugEnabled()) {
        try {
          let diag = (globalThis as Record<string, unknown>).__keel_drain_diag as
            | string[]
            | undefined;
          if (!diag) {
            diag = [];
            (globalThis as Record<string, unknown>).__keel_drain_diag = diag;
          }
          diag.push(`${Date.now()}:[keel:reconciler] flush ops=${this.operations.length}`);
          if (diag.length > DIAG_BUFFER_LIMIT) {
            diag.splice(0, diag.length - DIAG_BUFFER_LIMIT);
          }
        } catch {
          /* ignore */
        }
      }

      if (this.operations.length === 0) {
        if (this.isDebugEnabled()) {
          console.warn('[keel:reconciler] flush called with 0 ops');
        }
        return;
      }

      const opLen = this.operations.length;
      const debug = this.isDebugEnabled();
      if (debug) console.log('[keel:reconciler] flush ops=', opLen);

      // Keep the console usable: no per-op detail unless debug mode asked for it
      if (debug) {
        try {
          if (opLen <= 30) {
            console.log('[keel:reconciler] ops detail', JSON.stringify(this.operations));
          } else {
            const opCounts: Record<string, number> = {};
            for (const op of this.operations) {
              opCounts[op.op] = (opCounts[op.op] || 0) + 1;
            }
            console.log('[keel:reconciler] opCounts', JSON.stringify(opCounts));
            console.log('[keel:reconciler] ops head', JSON.stringify(this.operations.slice(0, 5)));
          }
        } catch {
          // ignore
        }
      }

      const batch: SerializedOperationBatch = {
        version: this.version,
        batchId: ++this.batchId,
        operations: [...this.operations],
      };

      this.operations = [];

      // JSON-serialize the batch before crossing the JSI boundary.
      // On Android (and other native sandbox hosts), the JSI bridge recursively
      // converts nested JS objects (qjsToJSI / sandboxToHost), which overflows
      // the native stack for large render batches. Passing a JSON string avoids
      // this: strings are primitive values in JSI — no recursive conversion.
      // The Host Bridge detects string input and JSON.parse's it back.
      sendToHost(JSON.stringify(batch));
    } finally {
      this._flushing = false;
    }
  }

  /**
   * Get pending operation count
   */
  get pendingCount(): number {
    return this.operations.length;
  }
}
