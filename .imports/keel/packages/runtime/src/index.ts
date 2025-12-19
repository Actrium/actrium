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
} from './engine';
// Core exports
export { Engine } from './engine';

// JSEngineProvider interface is provided by @rill/sandbox
// Platform-specific implementations: @rill/sandbox-native (JSC/QuickJS), @rill/sandbox-web (Worker)

// Note: DefaultComponents and EngineView are in presets (presets/react-native, presets/react-web)
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
  EngineViewPropsBase,
} from './types';
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
export type { SendToSandbox } from './receiver';
export { Receiver } from './receiver';
export type { ComponentMap, ComponentType } from './registry';
export { ComponentRegistry, createRegistry } from './registry';
