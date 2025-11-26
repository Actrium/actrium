/**
 * Rill Core Type Definitions
 */

// ============ Operation Types (Sandbox → Host) ============

/**
 * Operation type enum
 */
export type OperationType =
  | 'CREATE'
  | 'UPDATE'
  | 'DELETE'
  | 'APPEND'
  | 'INSERT'
  | 'REMOVE'
  | 'REORDER'
  | 'TEXT';

/**
 * Base operation interface
 */
export interface BaseOperation {
  op: OperationType;
  id: number;
  timestamp?: number;
}

/**
 * Create node operation
 */
export interface CreateOperation extends BaseOperation {
  op: 'CREATE';
  type: string;
  props: SerializedProps;
}

/**
 * Update node operation
 */
export interface UpdateOperation extends BaseOperation {
  op: 'UPDATE';
  props: SerializedProps;
  removedProps?: string[];
}

/**
 * Append child node operation
 */
export interface AppendOperation extends BaseOperation {
  op: 'APPEND';
  parentId: number;
  childId: number;
}

/**
 * Insert child node operation
 */
export interface InsertOperation extends BaseOperation {
  op: 'INSERT';
  parentId: number;
  childId: number;
  index: number;
}

/**
 * Delete node operation
 */
export interface DeleteOperation extends BaseOperation {
  op: 'DELETE';
}

/**
 * Remove child node operation
 */
export interface RemoveOperation extends BaseOperation {
  op: 'REMOVE';
  parentId: number;
  childId: number;
}

/**
 * Reorder children operation
 */
export interface ReorderOperation extends BaseOperation {
  op: 'REORDER';
  parentId: number;
  childIds: number[];
}

/**
 * Update text operation
 */
export interface TextOperation extends BaseOperation {
  op: 'TEXT';
  text: string;
}

export type Operation =
  | CreateOperation
  | UpdateOperation
  | AppendOperation
  | InsertOperation
  | DeleteOperation
  | RemoveOperation
  | ReorderOperation
  | TextOperation;

// ============ Property Serialization ============

/**
 * Serialized props object
 */
export type SerializedProps = Record<string, SerializedValue>;

/**
 * Serialized value type
 */
export type SerializedValue =
  | null
  | boolean
  | number
  | string
  | SerializedFunction
  | SerializedValue[]
  | { [key: string]: SerializedValue };

/**
 * Serialized function reference
 */
export interface SerializedFunction {
  __type: 'function';
  __fnId: string;
}

// ============ Host Message Types (Host → Sandbox) ============

/**
 * Host message type enum
 */
export type HostMessageType =
  | 'CALL_FUNCTION'
  | 'HOST_EVENT'
  | 'CONFIG_UPDATE'
  | 'DESTROY';

/**
 * Base message interface
 */
export interface BaseHostMessage {
  type: HostMessageType;
  seq?: number;
}

/**
 * Call callback function message
 */
export interface CallFunctionMessage extends BaseHostMessage {
  type: 'CALL_FUNCTION';
  fnId: string;
  args: SerializedValue[];
}

/**
 * Host event message
 */
export interface HostEventMessage extends BaseHostMessage {
  type: 'HOST_EVENT';
  eventName: string;
  payload: SerializedValue;
}

/**
 * Config update message
 */
export interface ConfigUpdateMessage extends BaseHostMessage {
  type: 'CONFIG_UPDATE';
  config: Record<string, SerializedValue>;
}

/**
 * Destroy message
 */
export interface DestroyMessage extends BaseHostMessage {
  type: 'DESTROY';
}

export type HostMessage =
  | CallFunctionMessage
  | HostEventMessage
  | ConfigUpdateMessage
  | DestroyMessage;

// ============ Batch Updates ============

/**
 * Operation batch
 */
export interface OperationBatch {
  version: number;
  batchId: number;
  operations: Operation[];
}

/**
 * Send function type
 */
export type SendToHost = (batch: OperationBatch) => void;

// ============ Virtual Node ============

/**
 * Virtual node (internal representation)
 */
export interface VNode {
  id: number;
  type: string;
  props: Record<string, unknown>;
  children: VNode[];
  parent: VNode | null;
}

/**
 * Node instance (host side)
 */
export interface NodeInstance {
  id: number;
  type: string;
  props: Record<string, unknown>;
  children: number[];
}

// ============ Style Types ============

/**
 * Flexbox direction
 */
export type FlexDirection = 'row' | 'column' | 'row-reverse' | 'column-reverse';

/**
 * Flexbox alignment
 */
export type FlexAlign =
  | 'flex-start'
  | 'flex-end'
  | 'center'
  | 'stretch'
  | 'baseline';

/**
 * Flexbox justify
 */
export type FlexJustify =
  | 'flex-start'
  | 'flex-end'
  | 'center'
  | 'space-between'
  | 'space-around'
  | 'space-evenly';

/**
 * Style object type
 */
export interface StyleObject {
  // Layout
  flex?: number;
  flexDirection?: FlexDirection;
  justifyContent?: FlexJustify;
  alignItems?: FlexAlign;
  alignSelf?: FlexAlign;
  flexWrap?: 'wrap' | 'nowrap' | 'wrap-reverse';
  flexGrow?: number;
  flexShrink?: number;
  flexBasis?: number | string;

  // Spacing
  padding?: number;
  paddingTop?: number;
  paddingRight?: number;
  paddingBottom?: number;
  paddingLeft?: number;
  paddingHorizontal?: number;
  paddingVertical?: number;
  margin?: number;
  marginTop?: number;
  marginRight?: number;
  marginBottom?: number;
  marginLeft?: number;
  marginHorizontal?: number;
  marginVertical?: number;

  // Size
  width?: number | string;
  height?: number | string;
  minWidth?: number | string;
  minHeight?: number | string;
  maxWidth?: number | string;
  maxHeight?: number | string;

  // Position
  position?: 'relative' | 'absolute';
  top?: number;
  right?: number;
  bottom?: number;
  left?: number;
  zIndex?: number;

  // Background
  backgroundColor?: string;
  opacity?: number;

  // Border
  borderWidth?: number;
  borderTopWidth?: number;
  borderRightWidth?: number;
  borderBottomWidth?: number;
  borderLeftWidth?: number;
  borderColor?: string;
  borderRadius?: number;
  borderTopLeftRadius?: number;
  borderTopRightRadius?: number;
  borderBottomLeftRadius?: number;
  borderBottomRightRadius?: number;

  // Text
  color?: string;
  fontSize?: number;
  fontWeight?:
    | 'normal'
    | 'bold'
    | '100'
    | '200'
    | '300'
    | '400'
    | '500'
    | '600'
    | '700'
    | '800'
    | '900';
  fontFamily?: string;
  fontStyle?: 'normal' | 'italic';
  textAlign?: 'auto' | 'left' | 'right' | 'center' | 'justify';
  textDecorationLine?: 'none' | 'underline' | 'line-through';
  lineHeight?: number;
  letterSpacing?: number;

  // Shadow (iOS)
  shadowColor?: string;
  shadowOffset?: { width: number; height: number };
  shadowOpacity?: number;
  shadowRadius?: number;

  // Elevation (Android)
  elevation?: number;

  // Transform
  transform?: Array<
    | { translateX: number }
    | { translateY: number }
    | { scale: number }
    | { scaleX: number }
    | { scaleY: number }
    | { rotate: string }
    | { rotateX: string }
    | { rotateY: string }
    | { rotateZ: string }
    | { skewX: string }
    | { skewY: string }
  >;

  // Other
  overflow?: 'visible' | 'hidden' | 'scroll';
}

/**
 * Style prop type
 */
export type StyleProp = StyleObject | StyleObject[] | undefined;
