/**
 * Operation Collector
 * Collects operations during render phase, sends all during commit phase
 */

import type { SendToHost, SerializedOperation, SerializedOperationBatch } from '../types';

export class OperationCollector {
  private operations: SerializedOperation[] = [];
  private batchId = 0;
  private version = 1;

  private isDebugEnabled(): boolean {
    try {
      return Boolean((globalThis as Record<string, unknown>).__RILL_RECONCILER_DEBUG__);
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

    // 调试：少量输出，便于确认是否有 CREATE/APPEND/UPDATE 被收集（默认关闭）
    if (this.isDebugEnabled()) {
      const len = this.operations.length;
      if (len <= 10 || len % 50 === 0) {
        console.log('[rill:reconciler] add op', op.op, 'len', len);
      }
    }
  }

  /**
   * Flush and send all operations
   */
  flush(sendToHost: SendToHost): void {
    if (this.operations.length === 0) {
      if (this.isDebugEnabled()) {
        console.warn('[rill:reconciler] flush called with 0 ops');
      }
      return;
    }

    const opLen = this.operations.length;
    const debug = this.isDebugEnabled();
    if (debug) console.log('[rill:reconciler] flush ops=', opLen);

    // Track operation counts using global variable for debugging
    const opCounts: Record<string, number> = {};
    this.operations.forEach((op) => {
      opCounts[op.op] = (opCounts[op.op] || 0) + 1;
    });
    globalThis.__OP_COUNTS = opCounts;
    globalThis.__TOTAL_OPS = opLen;

    // 避免刷屏/卡顿：默认不输出详细 ops（仅 debug 模式）
    if (debug) {
      try {
        if (opLen <= 30) {
          console.log('[rill:reconciler] ops detail', JSON.stringify(this.operations));
        } else {
          console.log('[rill:reconciler] opCounts', JSON.stringify(opCounts));
          console.log('[rill:reconciler] ops head', JSON.stringify(this.operations.slice(0, 5)));
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
    sendToHost(batch);
  }

  /**
   * Get pending operation count
   */
  get pendingCount(): number {
    return this.operations.length;
  }
}
