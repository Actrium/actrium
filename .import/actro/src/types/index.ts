/**
 * AskIt Types - Shared type definitions for Host and Guest
 */

import type { ReactNode } from 'react';

// ============================================================================
// Environment Detection
// ============================================================================

export type Environment = 'host' | 'guest';

// ============================================================================
// Style Types (React Native Compatible)
// ============================================================================

export type FlexDirection = 'row' | 'column' | 'row-reverse' | 'column-reverse';
export type FlexAlign = 'flex-start' | 'flex-end' | 'center' | 'stretch' | 'baseline';
export type FlexJustify =
  | 'flex-start'
  | 'flex-end'
  | 'center'
  | 'space-between'
  | 'space-around'
  | 'space-evenly';

export interface ViewStyle {
  // Layout
  flex?: number;
  flexDirection?: FlexDirection;
  justifyContent?: FlexJustify;
  alignItems?: FlexAlign;
  alignSelf?: FlexAlign;
  flexWrap?: 'wrap' | 'nowrap' | 'wrap-reverse';
  flexGrow?: number;
  flexShrink?: number;

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
  gap?: number;
  rowGap?: number;
  columnGap?: number;

  // Size
  width?: number | string;
  height?: number | string;
  minWidth?: number | string;
  maxWidth?: number | string;
  minHeight?: number | string;
  maxHeight?: number | string;

  // Position
  position?: 'relative' | 'absolute';
  top?: number | string;
  right?: number | string;
  bottom?: number | string;
  left?: number | string;
  zIndex?: number;

  // Appearance
  backgroundColor?: string;
  borderRadius?: number;
  borderTopLeftRadius?: number;
  borderTopRightRadius?: number;
  borderBottomLeftRadius?: number;
  borderBottomRightRadius?: number;
  borderWidth?: number;
  borderColor?: string;
  borderStyle?: 'solid' | 'dotted' | 'dashed';
  opacity?: number;
  overflow?: 'visible' | 'hidden' | 'scroll';

  // Shadow (iOS)
  shadowColor?: string;
  shadowOffset?: { width: number; height: number };
  shadowOpacity?: number;
  shadowRadius?: number;

  // Elevation (Android)
  elevation?: number;
}

export interface TextStyle extends ViewStyle {
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
  textDecorationLine?: 'none' | 'underline' | 'line-through' | 'underline line-through';
  lineHeight?: number;
  letterSpacing?: number;
}

export interface ImageStyle extends ViewStyle {
  resizeMode?: 'cover' | 'contain' | 'stretch' | 'repeat' | 'center';
  tintColor?: string;
}

// ============================================================================
// Common Component Props
// ============================================================================

export interface BaseProps {
  style?: ViewStyle;
  testID?: string;
}

// ============================================================================
// StepList Component Types
// ============================================================================

export type StepStatus = 'pending' | 'active' | 'completed' | 'error';

export interface StepItem {
  id: string;
  title: string;
  subtitle?: string;
  status: StepStatus;
  icon?: string;
}

export interface StepListProps extends BaseProps {
  items: StepItem[];
  loop?: boolean;
  onStepPress?: (item: StepItem, index: number) => void;
  activeColor?: string;
  completedColor?: string;
  pendingColor?: string;
  errorColor?: string;
  lineWidth?: number;
}

// ============================================================================
// ThemeView Component Types
// ============================================================================

export interface ThemeViewProps extends BaseProps {
  children?: ReactNode;
  variant?: 'primary' | 'secondary' | 'surface' | 'background';
  padding?: number | 'none' | 'small' | 'medium' | 'large';
}

// ============================================================================
// UserAvatar Component Types
// ============================================================================

export interface UserAvatarProps extends BaseProps {
  uri?: string;
  name?: string;
  size?: number | 'small' | 'medium' | 'large';
  showOnlineStatus?: boolean;
  isOnline?: boolean;
  onPress?: () => void;
}

// ============================================================================
// ChatBubble Component Types
// ============================================================================

export interface ChatBubbleProps extends BaseProps {
  children?: ReactNode;
  content?: string;
  isOwn?: boolean;
  showTail?: boolean;
  timestamp?: string | number | Date;
  status?: 'sending' | 'sent' | 'delivered' | 'read' | 'error';
  onLongPress?: () => void;
  onPress?: () => void;
  renderMarkdown?: boolean;
}

// ============================================================================
// Message Protocol Types - Type-safe payload mapping
// ============================================================================

/**
 * Type-safe message map for askit protocol
 * Maps event names to their expected payload types
 */
export interface AskitMessageMap {
  // Toast module
  'askit:toast:show': [message: string, options?: ToastOptions];

  // Haptic module
  'askit:haptic:trigger': [type?: HapticType];

  // EventEmitter events (using event: prefix)
  // Note: Event emitter events are dynamic, so we use string index signature
}

/**
 * EventEmitter message map - allows any event with any payload
 */
export interface EventEmitterMessageMap {
  [event: string]: unknown;
}

/**
 * Type-safe sendToHost function signature
 */
export interface TypedSendToHost {
  <K extends keyof AskitMessageMap>(event: K, payload: AskitMessageMap[K]): void;
  // Allow askit:event: prefixed messages with any payload
  (event: `askit:event:${string}`, payload?: unknown): void;
  // Fallback for any string event
  (event: string, payload?: unknown): void;
}

// ============================================================================
// EventEmitter (Communication) Types
// ==================================

export type EventCallback<T = unknown> = (payload: T) => void;

/**
 * Rate limiting options for event listeners
 */
export interface EventListenerOptions {
  /**
   * Rate limiting type
   * - 'throttle': Execute at most once per delay period
   * - 'debounce': Execute only after events stop for delay period
   * - 'none': No rate limiting (default)
   */
  rateLimit?: 'throttle' | 'debounce' | 'none';

  /**
   * Delay in milliseconds for rate limiting (default: 100ms)
   * Keep this value low (50-200ms) to maintain responsiveness
   */
  delay?: number;
}

/**
 * Typed event map for askit EventEmitter
 * Similar to Rill's EngineEvents, provides type safety for event payload
 *
 * Users can extend this interface to add custom typed events:
 *
 * @example
 * ```typescript
 * declare module 'askit' {
 *   interface AskitEvents {
 *     'user:login': { userId: string; timestamp: number };
 *     'user:logout': { userId: string };
 *     'analytics:click': { buttonId: string; metadata?: Record<string, unknown> };
 *   }
 * }
 * ```
 */
// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface AskitEvents {
  // Empty by default - users can extend via module augmentation
}

/**
 * Type-safe EventEmitter API
 *
 * Supports both typed events (via AskitEvents interface) and dynamic events.
 * When using typed events, payload types are enforced. For dynamic events,
 * you can specify the payload type via generic parameter.
 */
export interface EventEmitterAPI {
  /**
   * Emit a typed event
   */
  emit<K extends keyof AskitEvents>(event: K, payload: AskitEvents[K]): void;
  /**
   * Emit a dynamic event with optional payload type
   */
  emit<T = unknown>(event: string, payload?: T): void;

  /**
   * Subscribe to a typed event
   */
  on<K extends keyof AskitEvents>(
    event: K,
    callback: EventCallback<AskitEvents[K]>,
    options?: EventListenerOptions
  ): () => void;
  /**
   * Subscribe to a dynamic event or pattern
   */
  on<T = unknown>(
    event: string,
    callback: EventCallback<T>,
    options?: EventListenerOptions
  ): () => void;

  /**
   * Unsubscribe from a typed event
   */
  off<K extends keyof AskitEvents>(event: K, callback: EventCallback<AskitEvents[K]>): void;
  /**
   * Unsubscribe from a dynamic event or pattern
   */
  off<T = unknown>(event: string, callback: EventCallback<T>): void;

  /**
   * Subscribe to a typed event once
   */
  once<K extends keyof AskitEvents>(event: K, callback: EventCallback<AskitEvents[K]>): () => void;
  /**
   * Subscribe to a dynamic event or pattern once
   */
  once<T = unknown>(event: string, callback: EventCallback<T>): () => void;

  /**
   * Remove all listeners for an event, or all listeners if no event specified
   */
  removeAllListeners(event?: string): void;
}

// ============================================================================
// Toast Types
// ============================================================================

export type ToastPosition = 'top' | 'center' | 'bottom';
export type ToastDuration = 'short' | 'long' | number;

export interface ToastOptions {
  position?: ToastPosition;
  duration?: ToastDuration;
}

export interface ToastAPI {
  show: (message: string, options?: ToastOptions) => void;
}

// ============================================================================
// Haptic Types
// ============================================================================

export type HapticType =
  | 'light'
  | 'medium'
  | 'heavy'
  | 'selection'
  | 'success'
  | 'warning'
  | 'error';

export interface HapticAPI {
  trigger: (type?: HapticType) => void;
}

// ============================================================================
// Core Types (Host Only)
// ============================================================================

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type ComponentType = React.ComponentType<any>;
export type ComponentMap = Record<string, ComponentType>;
export type ModuleMap = Record<string, unknown>;

export interface AskitRegistryConfig {
  components: ComponentMap;
  modules: ModuleMap;
}
