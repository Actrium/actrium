/**
 * Rill DevTools Types
 *
 * Shared type definitions for Host and Guest communication
 */

// ============ Common Types ============

export type Timestamp = number;
export type NodeId = number;

// ============ Host-side Types ============

/**
 * Host component tree node (rendered in native)
 */
export interface HostTreeNode {
  id: NodeId;
  type: string;
  props: Record<string, unknown>;
  children: HostTreeNode[];
  depth: number;
}

/**
 * Operation log entry
 */
export interface OperationLogEntry {
  batchId: number;
  timestamp: Timestamp;
  operations: OperationRecord[];
  duration?: number | undefined;
}

export interface OperationRecord {
  op: 'CREATE' | 'UPDATE' | 'DELETE' | 'APPEND' | 'INSERT' | 'REMOVE' | 'REORDER' | 'TEXT';
  id?: NodeId | undefined;
  type?: string | undefined;
  props?: Record<string, unknown> | undefined;
  parentId?: NodeId | undefined;
}

/**
 * Host performance metrics
 */
export interface HostMetrics {
  operationProcessingTime: number;
  pendingOperations: number;
  nodeCount: number;
  memoryUsage?: number | undefined;
}

/**
 * Sandbox status
 */
export interface SandboxStatus {
  state: 'idle' | 'running' | 'ready' | 'error' | 'destroyed';
  startTime?: Timestamp;
  lastActivityTime?: Timestamp;
  errorCount: number;
}

// ============ Guest-side Types ============

/**
 * Console log entry
 */
export interface ConsoleEntry {
  level: 'log' | 'info' | 'warn' | 'error' | 'debug';
  args: unknown[];
  timestamp: Timestamp;
  stack?: string | undefined;
}

/**
 * Guest error
 */
export interface GuestError {
  message: string;
  stack?: string | undefined;
  timestamp: Timestamp;
  fatal: boolean;
}

// ============ Bridge Message Types ============

/**
 * Reconciler render timing (uses numeric nodeId matching VNode)
 */
export interface ReconcilerRenderTiming {
  nodeId: NodeId;
  type: string;
  phase: 'mount' | 'update';
  duration: number;
  timestamp: Timestamp;
}

/**
 * Guest → Host debug messages
 */
export type GuestDebugMessage =
  | { type: '__DEVTOOLS_CONSOLE__'; entry: ConsoleEntry }
  | { type: '__DEVTOOLS_ERROR__'; error: GuestError }
  | {
      type: '__DEVTOOLS_RENDER__';
      timings: ReconcilerRenderTiming[];
      commitDuration: number;
      timestamp: Timestamp;
    }
  | { type: '__DEVTOOLS_READY__' };

// ============ Event Types ============

export type DevToolsEventType = 'operation' | 'console' | 'error' | 'render';

export interface DevToolsEvent {
  type: DevToolsEventType;
  timestamp: Timestamp;
  data: unknown;
}

export type EventHandler<T = unknown> = (event: DevToolsEvent & { data: T }) => void;
export type Unsubscribe = () => void;

// ============ Profiling Types ============

export interface ProfilingSession {
  startTime: Timestamp;
  endTime?: Timestamp;
  hostOperations: OperationLogEntry[];
  guestRenders: ReconcilerRenderTiming[];
  errors: GuestError[];
}

export interface ProfilingReport {
  duration: number;
  summary: {
    totalOperations: number;
    totalRenders: number;
    avgOperationTime: number;
    avgRenderTime: number;
    slowestNodes: Array<{ nodeId: NodeId; type: string; avgTime: number; count: number }>;
    errorCount: number;
  };
  timeline: Array<{
    timestamp: Timestamp;
    type: 'operation' | 'render' | 'error';
    data: unknown;
  }>;
}
