/**
 * keel/guest Type Definitions
 *
 * Types for Guest-side SDK and Reconciler
 * Communication-related types are re-exported from bridge/types.ts
 */

import type React from 'react';
import type { ReviewedUnknown } from '../shared';

// ============================================
// Re-exported communication types from Bridge
// ============================================

export type {
  AppendOperation,
  BaseOperation,
  BridgeValueObject,
  CallbackRegistry,
  CreateOperation,
  DeleteOperation,
  InsertOperation,
  Operation,
  OperationBatch,
  // Operation types
  OperationType,
  RemoveOperation,
  ReorderOperation,
  // Function types
  SendToHost,
  // Serialized operation types (used on the Guest side)
  SerializedCreateOperation,
  SerializedFunction,
  SerializedOperation,
  SerializedOperationBatch,
  SerializedProps,
  SerializedUpdateOperation,
  // Serialized types
  SerializedValue,
  SerializedValueObject,
  TextOperation,
  UpdateOperation,
} from '../shared';
// Re-export type guards from Bridge
export { isSerializedFunction, operationHasProps } from '../shared';

// ============================================
// Guest Element Types (let-specific)
// ============================================

/**
 * Guest component reference (registered on Host)
 */
export interface GuestComponentRef {
  __keelComponentId: string;
  displayName?: string;
}

/**
 * Guest React element (from sandbox)
 */
export interface GuestReactElement {
  __keelTypeMarker?: string;
  __keelFragmentType?: string;
  $$typeof?: symbol;
  // biome-ignore lint/complexity/noBannedTypes: React element type can be Function for component references
  type: string | symbol | Function | GuestComponentRef | Record<string, unknown>;
  props?: Record<string, unknown>;
  key?: React.Key | null;
  ref?: ReviewedUnknown;
  children?: ReviewedUnknown;
}

/**
 * Valid Guest element types
 * More type-safe than `unknown` - explicitly defines what can come from sandbox
 */
export type GuestElement =
  | null
  | undefined
  | string
  | number
  | boolean
  | GuestReactElement
  | GuestElement[];

/**
 * Type guard for Guest React element
 */
// Reason: Type guard input is untrusted (sandbox boundary)
export function isGuestReactElement(value: unknown): value is GuestReactElement {
  if (typeof value !== 'object' || value === null) {
    return false;
  }

  const el = value as Record<string, unknown>;

  // Check for Keel markers
  if (el.__keelTypeMarker === '__keel_react_element__') {
    return true;
  }

  // Check for React $$typeof symbol
  if (typeof el.$$typeof === 'symbol') {
    return true;
  }

  // Heuristic: has type and props
  if ('type' in el && 'props' in el) {
    return true;
  }

  return false;
}

/**
 * Type guard for Guest component reference
 */
// Reason: Type guard input is untrusted (sandbox boundary)
export function isGuestComponentRef(value: unknown): value is GuestComponentRef {
  return (
    typeof value === 'object' &&
    value !== null &&
    '__keelComponentId' in value &&
    typeof (value as GuestComponentRef).__keelComponentId === 'string'
  );
}

// ============================================
// Virtual Node (used internally by let)
// ============================================

/**
 * Virtual node (internal representation)
 */
export interface VNode {
  id: number;
  type: string;
  props: Record<string, unknown>;
  children: VNode[];
  parent: VNode | null;
  /** Registered callback function IDs for cleanup */
  registeredFnIds?: Set<string>;
}

// ============================================
// Style Types (from shared style-types.ts)
// ============================================

export type {
  // Value types
  ColorValue,
  DimensionValue,
  // Flex types
  FlexAlign,
  FlexDirection,
  FlexJustify,
  FlexStyle,
  GestureResponderEvent,
  ImageSource,
  ImageStyle,
  LayoutChangeEvent,
  // Event types
  LayoutEvent,
  NativeSyntheticEvent,
  ScrollEvent,
  // Style types
  StyleObject,
  StyleProp,
  TextLayoutEvent,
  TextLayoutLine,
  TextStyle,
  ViewStyle,
} from '../shared/style-types';

// ============================================
// Remote Ref Types (cross-sandbox component reference)
// ============================================

/**
 * Remote Ref - the Guest side's reference to a Host component instance
 *
 * Sandbox isolation means a guest cannot reach a host native component instance directly.
 * RemoteRef gives it an async call mechanism instead: host-side methods are invoked by message.
 *
 * @example
 * const [inputRef, remoteInput] = useRemoteRef<TextInputRef>();
 * <TextInput ref={inputRef} />
 * await remoteInput?.invoke('focus');
 */
export interface RemoteRef<T = unknown> {
  /** Node ID */
  readonly nodeId: number;

  /**
   * Invoke a Host-side component method
   * @param method - method name
   * @param args - method arguments
   * @returns the return value wrapped in a Promise
   */
  invoke<R = unknown>(method: string, ...args: unknown[]): Promise<R>;

  /**
   * Type-safe method-call proxy
   * Implemented with a Proxy, so any method name called on it becomes an invoke() call.
   */
  call: T extends Record<string, (...args: unknown[]) => unknown>
    ? { [K in keyof T]: (...args: Parameters<T[K]>) => Promise<Awaited<ReturnType<T[K]>>> }
    : Record<string, (...args: unknown[]) => Promise<unknown>>;
}

/**
 * Measure result
 */
export interface MeasureResult {
  x: number;
  y: number;
  width: number;
  height: number;
  pageX: number;
  pageY: number;
}

/**
 * Methods for components whose layout can be measured
 */
export interface MeasurableRef {
  measure(): Promise<MeasureResult>;
  measureInWindow(): Promise<{
    x: number;
    y: number;
    width: number;
    height: number;
  }>;
}

/**
 * TextInput component methods
 */
export interface TextInputRef extends MeasurableRef {
  focus(): Promise<void>;
  blur(): Promise<void>;
  clear(): Promise<void>;
  isFocused(): Promise<boolean>;
  setNativeProps(props: Record<string, unknown>): Promise<void>;
}

/**
 * ScrollView component methods
 */
export interface ScrollViewRef extends MeasurableRef {
  scrollTo(options: { x?: number; y?: number; animated?: boolean }): Promise<void>;
  scrollToEnd(options?: { animated?: boolean }): Promise<void>;
  flashScrollIndicators(): Promise<void>;
}

/**
 * FlatList component methods
 */
export interface FlatListRef extends ScrollViewRef {
  scrollToIndex(params: {
    index: number;
    animated?: boolean;
    viewOffset?: number;
    viewPosition?: number;
  }): Promise<void>;
  scrollToItem(params: {
    item: ReviewedUnknown;
    animated?: boolean;
    viewPosition?: number;
  }): Promise<void>;
  scrollToOffset(params: { offset: number; animated?: boolean }): Promise<void>;
  recordInteraction(): Promise<void>;
}

/**
 * Remote Ref callback type
 * Used for the ref callback returned by useRemoteRef
 */
export type RemoteRefCallback = (instance: { nodeId: number } | null) => void;
