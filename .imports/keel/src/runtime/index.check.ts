/**
 * Rill Runtime (for type checking only)
 *
 * 宿主端运行时，负责沙箱管理、指令接收、UI 渲染
 */

// 核心导出
export { Engine } from './engine';
export type { EngineOptions, EngineEvents } from './engine';

// EngineView 需要 React Native 环境，在 check 中排除
// export { EngineView } from './EngineView';
// export type { EngineViewProps } from './EngineView';

export { Receiver } from './receiver';
export type { SendToSandbox } from './receiver';

export { ComponentRegistry, createRegistry } from './registry';
export type { ComponentType, ComponentMap } from './registry';

// 性能优化
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

// 类型导出
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
