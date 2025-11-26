/**
 * Rill Runtime
 *
 * Host-side runtime, responsible for sandbox management, operation receiving, and UI rendering
 */

// Core exports
export { Engine } from './engine';
export type { EngineOptions, EngineEvents } from './engine';

export { EngineView } from './EngineView';
export type { EngineViewProps } from './EngineView';

export { Receiver } from './receiver';
export type { SendToSandbox } from './receiver';

export { ComponentRegistry, createRegistry } from './registry';
export type { ComponentType, ComponentMap } from './registry';

// Performance optimization
export {
  OperationMerger,
  ThrottledScheduler,
  VirtualScrollCalculator,
  ScrollThrottler,
  PerformanceMonitor,
} from './performance';
export type {
  BatchConfig,
  VirtualScrollConfig,
  VirtualScrollState,
  PerformanceMetrics,
} from './performance';

// Type exports
export type {
  Operation,
  OperationBatch,
  OperationType,
  CreateOperation,
  UpdateOperation,
  DeleteOperation,
  AppendOperation,
  InsertOperation,
  RemoveOperation,
  ReorderOperation,
  TextOperation,
  HostMessage,
  HostMessageType,
  CallFunctionMessage,
  HostEventMessage,
  ConfigUpdateMessage,
  DestroyMessage,
  SerializedProps,
  SerializedValue,
  SerializedFunction,
  NodeInstance,
  StyleObject,
  StyleProp,
} from '../types';

// Default components (need to be implemented in components/ directory)
// export { DefaultComponents } from '../components';
