/**
 * Rill Runtime (for type checking only)
 *
 * Host-side runtime for sandbox management, instruction receiving, and UI rendering
 */

export type { EngineEvents, EngineOptions } from './engine';
// 核心导出
export { Engine } from './engine';

// EngineView 需要 React Native 环境，在 check 中排除
// export { EngineView } from './EngineView';
// export type { EngineViewProps } from './EngineView';

// 类型导出
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
} from '../types';
export type {
  BatchConfig,
  PerformanceMetrics,
  VirtualScrollConfig,
  VirtualScrollState,
} from './performance';
// 性能优化
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
