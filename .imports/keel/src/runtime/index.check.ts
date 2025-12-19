/**
 * Rill Runtime (for type checking only)
 *
 * Host-side runtime for sandbox management, instruction receiving, and UI rendering
 */

export type { EngineEvents, EngineOptions } from './engine';
// Core exports
export { Engine } from './engine';

// EngineView requires React Native environment, excluded in check
// export { EngineView } from './EngineView';
// export type { EngineViewProps } from './EngineView';

// Type exports
export type {
  AppendOperation,
  CallFunctionMessage,
  ConfigUpdateMessage,
  CreateOperation,
  DeleteOperation,
  DestroyMessage,
  HostEventMessage,
  HostMessage,
  HostMessageType,
  InsertOperation,
  NodeInstance,
  Operation,
  OperationBatch,
  OperationType,
  RemoveOperation,
  ReorderOperation,
  SerializedFunction,
  SerializedProps,
  SerializedValue,
  StyleObject,
  StyleProp,
  TextOperation,
  UpdateOperation,
} from './types';
export type {
  BatchConfig,
  PerformanceMetrics,
  VirtualScrollConfig,
  VirtualScrollState,
} from './performance';
// Performance optimizations
export {
  OperationMerger,
  PerformanceMonitor,
  ScrollThrottler,
  ThrottledScheduler,
  VirtualScrollCalculator,
} from './performance';
export type { SendToSandbox } from './receiver';
export { Receiver } from './receiver';
export type { ComponentMap, ComponentType } from './registry';
export { ComponentRegistry, createRegistry } from './registry';
