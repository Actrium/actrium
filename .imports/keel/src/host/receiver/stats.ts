/**
 * Receiver Statistics & Attribution Tracking
 *
 * Handles performance metrics, attribution window computation, and worst-batch analysis.
 */

import type {
  ReceiverApplySample,
  ReceiverApplyStats,
  ReceiverAttributionWindow,
  ReceiverAttributionWorstBatch,
} from './types';

// ============================================
// Attribution Tracker
// ============================================

/**
 * Attribution tracker for performance analysis
 */
export class AttributionTracker {
  private applyHistory: ReceiverApplySample[] = [];
  private attributionWindowMs: number;
  private attributionHistoryMs: number;
  private attributionMaxSamples: number;

  constructor(windowMs = 5000, historyMs = 60_000, maxSamples = 200) {
    this.attributionWindowMs = windowMs;
    this.attributionHistoryMs = Math.max(historyMs, windowMs);
    this.attributionMaxSamples = maxSamples;
  }

  /**
   * Record a batch apply sample
   */
  recordSample(stats: ReceiverApplyStats): void {
    // Store as ReceiverApplySample (same structure, just typed differently for internal use)
    this.applyHistory.push(stats as ReceiverApplySample);
    this.trimHistory(stats.at);
  }

  /**
   * Trim old history samples
   */
  private trimHistory(now: number): void {
    const cutoff = now - this.attributionHistoryMs;
    while (this.applyHistory.length > 0 && this.applyHistory[0]!.at < cutoff) {
      this.applyHistory.shift();
    }
    if (this.applyHistory.length > this.attributionMaxSamples) {
      this.applyHistory.splice(0, this.applyHistory.length - this.attributionMaxSamples);
    }
  }

  /**
   * Compute attribution window aggregation
   */
  computeWindow(now: number): ReceiverAttributionWindow | null {
    if (this.applyHistory.length === 0) return null;
    const cutoff = now - this.attributionWindowMs;
    const samples = this.applyHistory.filter((s) => s.at >= cutoff);
    if (samples.length === 0) return null;

    // Helper functions for aggregation
    const incRecord = (rec: Record<string, number>, key: string, n = 1) => {
      rec[key] = (rec[key] ?? 0) + n;
    };
    const incMap = (m: Map<string, number>, key: string, n = 1) => {
      m.set(key, (m.get(key) ?? 0) + n);
    };
    const topN = (m: Map<string, number>, n: number) =>
      Array.from(m.entries())
        .sort((a, b) => b[1] - a[1])
        .slice(0, n)
        .map(([type, ops]) => ({ type, ops }));

    // Aggregate metrics
    let total = 0;
    let applied = 0;
    let skipped = 0;
    let failed = 0;
    let durationMs = 0;
    let nodeDelta = 0;

    const opCounts: Record<string, number> = {};
    const skippedOpCounts: Record<string, number> = {};
    const nodeTypeCounts = new Map<string, number>();
    const skippedNodeTypeCounts = new Map<string, number>();

    for (const s of samples) {
      total += s.total;
      applied += s.applied;
      skipped += s.skipped;
      failed += s.failed;
      durationMs += s.durationMs;
      nodeDelta += s.nodeDelta;

      for (const [k, v] of Object.entries(s.opCounts)) incRecord(opCounts, k, v);
      for (const [k, v] of Object.entries(s.skippedOpCounts)) incRecord(skippedOpCounts, k, v);

      for (const { type, ops } of s.topNodeTypes) incMap(nodeTypeCounts, type, ops);
      for (const { type, ops } of s.topNodeTypesSkipped) incMap(skippedNodeTypeCounts, type, ops);
    }

    // Find worst batches
    const pickMax = (metric: (s: ReceiverApplyStats) => number): ReceiverApplyStats | null => {
      let best: ReceiverApplyStats | null = null;
      let bestV = -Infinity;
      for (const s of samples) {
        const v = metric(s);
        if (v > bestV) {
          bestV = v;
          best = s;
        }
      }
      return best;
    };

    const worstBatches: ReceiverAttributionWorstBatch[] = [];

    const largest = pickMax((s) => s.total);
    if (largest) {
      worstBatches.push(createWorstBatch('largest', largest));
    }

    const slowest = pickMax((s) => s.durationMs);
    if (slowest && slowest.durationMs > 0) {
      worstBatches.push(createWorstBatch('slowest', slowest));
    }

    const mostSkipped = pickMax((s) => s.skipped);
    if (mostSkipped && mostSkipped.skipped > 0) {
      worstBatches.push(createWorstBatch('mostSkipped', mostSkipped));
    }

    const mostGrowth = pickMax((s) => s.nodeDelta);
    if (mostGrowth && mostGrowth.nodeDelta !== 0) {
      worstBatches.push(createWorstBatch('mostGrowth', mostGrowth));
    }

    return {
      windowMs: this.attributionWindowMs,
      sampleCount: samples.length,
      total,
      applied,
      skipped,
      failed,
      durationMs,
      nodeDelta,
      opCounts,
      skippedOpCounts,
      topNodeTypes: topN(nodeTypeCounts, 6),
      topNodeTypesSkipped: topN(skippedNodeTypeCounts, 6),
      worstBatches,
    };
  }
}

// ============================================
// Helper Functions
// ============================================

/**
 * Create worst batch entry
 */
function createWorstBatch(
  kind: ReceiverAttributionWorstBatch['kind'],
  stats: ReceiverApplyStats
): ReceiverAttributionWorstBatch {
  return {
    kind,
    batchId: stats.batchId,
    at: stats.at,
    total: stats.total,
    applied: stats.applied,
    skipped: stats.skipped,
    failed: stats.failed,
    durationMs: stats.durationMs,
    nodeDelta: stats.nodeDelta,
  };
}
