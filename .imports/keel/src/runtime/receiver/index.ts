/**
 * Receiver Module
 *
 * Exports receiver types, utilities, and main Receiver class.
 */

// Re-export stats tracker
export { AttributionTracker } from './stats';
// Re-export types
export type {
  ReceiverApplySample,
  ReceiverApplyStats,
  ReceiverAttributionWindow,
  ReceiverAttributionWorstBatch,
  ReceiverAttributionWorstKind,
  ReceiverCallbackRegistry,
  ReceiverOptions,
  ReceiverStats,
  SendToSandbox,
} from './types';
// Re-export utilities
export { safeQueueMicrotask } from './types';

// TODO: Re-export main Receiver class when refactored
// export { Receiver } from './receiver';
