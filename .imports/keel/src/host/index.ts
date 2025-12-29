/**
 * @rill/runtime
 *
 * Host-side runtime for rill
 * Responsible for sandbox management, operation receiving, and UI rendering
 */

// Backward compatibility type aliases
export type {
  EngineActivityStats,
  EngineDiagnostics,
  EngineEvents,
  EngineHealth,
  EngineOptions,
  GuestMessage,
  JSEngineContext,
  JSEngineContext as QuickJSContext,
  JSEngineProvider,
  JSEngineProvider as QuickJSProvider,
  JSEngineRuntime,
  JSEngineRuntime as QuickJSRuntime,
} from './Engine';
// Core exports
export { Engine } from './Engine';

// JSEngineProvider interface is provided by @rill/sandbox
// Platform-specific implementations: @rill/sandbox-native (JSC/QuickJS), @rill/sandbox-web (Worker)

export type {
  BatchConfig,
  PerformanceMetrics,
  VirtualScrollConfig,
  VirtualScrollState,
} from './performance';
// Performance optimization
export {
  OperationMerger,
  PerformanceMonitor,
  ScrollThrottler,
  ThrottledScheduler,
  VirtualScrollCalculator,
} from './performance';
export type { DevToolsTreeNode, SendToSandbox } from './receiver';
export { Receiver } from './receiver';
export type { ComponentMap, ComponentType } from './registry';
export { ComponentRegistry, createRegistry } from './registry';
// Note: DefaultComponents and EngineView are in rill/host/preset
// Type exports
export type {
  AppendOperation,
  CallFunctionMessage,
  ConfigUpdateMessage,
  CreateOperation,
  DeleteOperation,
  DestroyMessage,
  EngineViewPropsBase,
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
// EngineView hook for custom EngineView implementations
export type { LoadingState, UseEngineViewOptions, UseEngineViewResult } from './useEngineView';
export { useEngineView } from './useEngineView';
