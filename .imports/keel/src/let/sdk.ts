/**
 * Rill SDK
 *
 * Virtual components and Hooks for Guest development
 * Zero runtime dependencies - all implementations injected by reconciler at runtime
 */

import type { ImageSource, LayoutEvent, ScrollEvent, StyleProp } from './types';

// ============ Runtime-injected React types ============
// These types mirror React's types but are defined locally to avoid React dependency

/** React node type - matches React.ReactNode */
export type ReactNode =
  | string
  | number
  | boolean
  | null
  | undefined
  | ReactNode[]
  | { $$typeof: symbol; type: unknown; props: unknown };

/** Component type - matches React.ComponentType */
export type ComponentType<P = object> =
  | ((props: P) => ReactNode)
  | { new (props: P): { render(): ReactNode } };

// ============ Runtime React hooks accessor ============

interface ReactHooks {
  useEffect: (effect: () => void | (() => void), deps?: unknown[]) => void;
  useRef: <T>(initial: T) => { current: T };
}

/**
 * Get React hooks from runtime-injected global or module system
 * React is injected by the rill reconciler when running in sandbox
 */
function getReactHooks(): ReactHooks {
  // 1. Try runtime-injected global (sandbox environment)
  const g = globalThis as { React?: ReactHooks };
  if (g.React && typeof g.React.useEffect === 'function') {
    return g.React as ReactHooks;
  }

  // 2. Try require (for testing/development outside sandbox)
  try {
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    const React = require('react');
    if (React && typeof React.useEffect === 'function') {
      return React as ReactHooks;
    }
  } catch {
    // React not available
  }

  // 3. Fallback: hooks not available (will fail at runtime if used)
  return {
    useEffect: () => {
      throw new Error('[rill/sdk] useEffect not available. Ensure running in rill sandbox.');
    },
    useRef: () => {
      throw new Error('[rill/sdk] useRef not available. Ensure running in rill sandbox.');
    },
  } as ReactHooks;
}

// ============ Virtual Component Definitions ============
// Used only as string identifiers at runtime

export const View = 'View';
export const Text = 'Text';
export const Image = 'Image';
export const ScrollView = 'ScrollView';
export const TouchableOpacity = 'TouchableOpacity';
export const FlatList = 'FlatList';
export const TextInput = 'TextInput';
export const Button = 'Button';
export const Switch = 'Switch';
export const ActivityIndicator = 'ActivityIndicator';

// ============ Component Props Type Definitions ============

/**
 * Common Props
 */
export interface BaseProps {
  style?: StyleProp;
  testID?: string;
  key?: string | number;
}

/**
 * View Component Props
 */
export interface ViewProps extends BaseProps {
  children?: ReactNode;
  onLayout?: (event: LayoutEvent) => void;
  pointerEvents?: 'auto' | 'none' | 'box-none' | 'box-only';
  accessible?: boolean;
  accessibilityLabel?: string;
}

/**
 * Text Component Props
 */
export interface TextProps extends BaseProps {
  children?: ReactNode;
  numberOfLines?: number;
  ellipsizeMode?: 'head' | 'middle' | 'tail' | 'clip';
  selectable?: boolean;
  onPress?: () => void;
  onLongPress?: () => void;
}

/**
 * Image Component Props
 */
export interface ImageProps extends BaseProps {
  source: ImageSource | ImageSource[];
  resizeMode?: 'cover' | 'contain' | 'stretch' | 'repeat' | 'center';
  onLoad?: () => void;
  onLoadStart?: () => void;
  onLoadEnd?: () => void;
  onError?: (error: { nativeEvent: { error: string } }) => void;
  fadeDuration?: number;
  blurRadius?: number;
}

/**
 * ScrollView Component Props
 */
export interface ScrollViewProps extends ViewProps {
  horizontal?: boolean;
  showsVerticalScrollIndicator?: boolean;
  showsHorizontalScrollIndicator?: boolean;
  pagingEnabled?: boolean;
  bounces?: boolean;
  scrollEnabled?: boolean;
  onScroll?: (event: ScrollEvent) => void;
  onScrollBeginDrag?: (event: ScrollEvent) => void;
  onScrollEndDrag?: (event: ScrollEvent) => void;
  onMomentumScrollBegin?: (event: ScrollEvent) => void;
  onMomentumScrollEnd?: (event: ScrollEvent) => void;
  scrollEventThrottle?: number;
  contentContainerStyle?: StyleProp;
  keyboardShouldPersistTaps?: 'always' | 'never' | 'handled';
  keyboardDismissMode?: 'none' | 'on-drag' | 'interactive';
}

/**
 * TouchableOpacity Component Props
 */
export interface TouchableOpacityProps extends BaseProps {
  children?: ReactNode;
  onPress?: () => void;
  onPressIn?: () => void;
  onPressOut?: () => void;
  onLongPress?: () => void;
  activeOpacity?: number;
  disabled?: boolean;
  delayPressIn?: number;
  delayPressOut?: number;
  delayLongPress?: number;
}

/**
 * FlatList Component Props
 */
export interface FlatListProps<T> extends ScrollViewProps {
  data: T[];
  renderItem: (info: { item: T; index: number }) => ReactNode;
  keyExtractor?: (item: T, index: number) => string;
  ItemSeparatorComponent?: ComponentType;
  ListHeaderComponent?: ReactNode;
  ListFooterComponent?: ReactNode;
  ListEmptyComponent?: ReactNode;
  numColumns?: number;
  initialNumToRender?: number;
  onEndReached?: () => void;
  onEndReachedThreshold?: number;
  refreshing?: boolean;
  onRefresh?: () => void;
  getItemLayout?: (
    data: T[] | null,
    index: number
  ) => {
    length: number;
    offset: number;
    index: number;
  };
}

/**
 * TextInput Component Props
 */
export interface TextInputProps extends BaseProps {
  value?: string;
  defaultValue?: string;
  placeholder?: string;
  placeholderTextColor?: string;
  onChangeText?: (text: string) => void;
  onChange?: (event: { nativeEvent: { text: string } }) => void;
  onFocus?: () => void;
  onBlur?: () => void;
  onSubmitEditing?: () => void;
  multiline?: boolean;
  numberOfLines?: number;
  maxLength?: number;
  editable?: boolean;
  autoCapitalize?: 'none' | 'sentences' | 'words' | 'characters';
  autoCorrect?: boolean;
  autoFocus?: boolean;
  secureTextEntry?: boolean;
  keyboardType?: 'default' | 'email-address' | 'numeric' | 'phone-pad' | 'decimal-pad' | 'url';
  returnKeyType?: 'done' | 'go' | 'next' | 'search' | 'send';
  selectTextOnFocus?: boolean;
}

/**
 * Button Component Props
 */
export interface ButtonProps {
  title: string;
  onPress: () => void;
  color?: string;
  disabled?: boolean;
  testID?: string;
  accessibilityLabel?: string;
}

/**
 * Switch Component Props
 */
export interface SwitchProps extends BaseProps {
  value?: boolean;
  onValueChange?: (value: boolean) => void;
  disabled?: boolean;
  trackColor?: { false?: string; true?: string };
  thumbColor?: string;
  ios_backgroundColor?: string;
}

/**
 * ActivityIndicator Component Props
 */
export interface ActivityIndicatorProps extends BaseProps {
  animating?: boolean;
  color?: string;
  size?: 'small' | 'large' | number;
  hidesWhenStopped?: boolean;
}

// ============ Event Types (re-exported from types.ts) ============
// LayoutEvent, ScrollEvent, ImageSource are now imported from '../types'

// ============ Hooks ============

/**
 * Subscribe to host events
 *
 * Automatically cleans up subscription when component unmounts.
 *
 * @param eventName Event name
 * @param callback Callback function
 *
 * @example
 * ```tsx
 * useHostEvent('REFRESH', () => {
 *   console.log('Host requested refresh');
 *   fetchData();
 * });
 * // Automatically unsubscribes when component unmounts
 * ```
 */
export function useHostEvent<T = unknown>(eventName: string, callback: (payload: T) => void): void {
  const { useEffect, useRef } = getReactHooks();

  // Use ref to avoid re-subscribing when callback changes
  const callbackRef = useRef(callback);

  // Keep ref up to date with latest callback
  useEffect(() => {
    callbackRef.current = callback;
  });

  useEffect(() => {
    const g = globalThis as Record<string, unknown>;
    if (typeof globalThis !== 'undefined' && '__useHostEvent' in globalThis) {
      // Create stable callback wrapper that always calls the latest callback
      const stableCallback = (payload: T) => callbackRef.current(payload);

      // Subscribe and get unsubscribe function
      const unsubscribe = (
        g.__useHostEvent as (name: string, cb: (payload: T) => void) => () => void
      )(eventName, stableCallback);

      // React automatically calls this cleanup function when component unmounts
      return unsubscribe;
    }
    return undefined;
  }, [eventName]); // Only re-subscribe when eventName changes
}

/**
 * Get initial configuration from host
 *
 * @returns Configuration object
 *
 * @example
 * ```tsx
 * const config = useConfig<{ theme: 'light' | 'dark' }>();
 * console.log(config.theme);
 * ```
 */
export function useConfig<T = Record<string, unknown>>(): T {
  // Actual implementation injected by reconciler at runtime
  const g = globalThis as Record<string, unknown>;
  if (typeof globalThis !== 'undefined' && '__getConfig' in globalThis) {
    return (g.__getConfig as () => T)();
  }
  return {} as T;
}

/**
 * Get function to send messages to host
 *
 * @returns send function
 *
 * @example
 * ```tsx
 * const sendToHost = useSendToHost();
 * sendToHost('ANALYTICS', { action: 'click', target: 'button' });
 * ```
 */
export function useSendToHost(): (eventName: string, payload?: unknown) => void {
  // Actual implementation injected by reconciler at runtime
  const g = globalThis as Record<string, unknown>;
  if (typeof globalThis !== 'undefined' && '__sendEventToHost' in globalThis) {
    return g.__sendEventToHost as (eventName: string, payload?: unknown) => void;
  }
  return () => {
    console.warn('[rill] sendToHost is not available outside sandbox');
  };
}

// ============ Error Boundary ============

/**
 * Error info passed to error handlers
 */
export interface ErrorInfo {
  componentStack: string;
}

/**
 * Props for RillErrorBoundary
 */
export interface RillErrorBoundaryProps {
  children: ReactNode;
  fallback?: ReactNode | ((error: Error, errorInfo: ErrorInfo) => ReactNode);
  onError?: (error: Error, errorInfo: ErrorInfo) => void;
}

/**
 * State for RillErrorBoundary
 */
interface RillErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
  errorInfo: ErrorInfo | null;
}

/**
 * Error Boundary component for catching render errors in Guest code
 *
 * @example
 * ```tsx
 * import { RillErrorBoundary, View, Text } from 'rill/sdk';
 *
 * function App() {
 *   return (
 *     <RillErrorBoundary
 *       fallback={<Text>Something went wrong</Text>}
 *       onError={(error, info) => {
 *         // Report error to host
 *         sendToHost('RENDER_ERROR', { message: error.message, stack: info.componentStack });
 *       }}
 *     >
 *       <MyComponent />
 *     </RillErrorBoundary>
 *   );
 * }
 * ```
 */
// Note: We use React.Component here because ErrorBoundary must be a class component
// Check that globalThis.React.Component is a valid constructor (not just an object from shims)
const React =
  typeof globalThis !== 'undefined' &&
  globalThis.React &&
  typeof (globalThis.React as { Component?: unknown }).Component === 'function'
    ? globalThis.React
    : { Component: class {} };

type ReactType = typeof React;
type ReactNodeType = ReactType extends { Component: { prototype: { render(): infer R } } }
  ? R
  : unknown;

export class RillErrorBoundary extends (React.Component as unknown as new (
  props: RillErrorBoundaryProps
) => {
  props: RillErrorBoundaryProps;
  state: RillErrorBoundaryState;
  setState: (state: Partial<RillErrorBoundaryState>) => void;
  render(): ReactNodeType;
}) {
  constructor(props: RillErrorBoundaryProps) {
    super(props);
    this.state = { hasError: false, error: null, errorInfo: null };
  }

  static getDerivedStateFromError(error: Error): Partial<RillErrorBoundaryState> {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: { componentStack: string }): void {
    const info: ErrorInfo = { componentStack: errorInfo.componentStack };
    this.setState({ errorInfo: info });

    // Call onError callback if provided
    if (this.props.onError) {
      this.props.onError(error, info);
    }

    // Also send to host if sendToHost is available
    const g = globalThis as Record<string, unknown>;
    if ('__sendEventToHost' in g) {
      const sendToHost = g.__sendEventToHost as (name: string, payload: unknown) => void;
      sendToHost('RENDER_ERROR', {
        message: error.message,
        stack: error.stack,
        componentStack: info.componentStack,
      });
    }
  }

  override render(): ReactNode {
    if (this.state.hasError) {
      const { fallback } = this.props;
      const { error, errorInfo } = this.state;

      if (typeof fallback === 'function' && error && errorInfo) {
        return fallback(error, errorInfo);
      }

      if (fallback && typeof fallback !== 'function') {
        return fallback;
      }

      // Default fallback - simple error message
      return null;
    }

    return this.props.children;
  }
}

// ============ Type Exports ============

export type { ImageSource, LayoutEvent, ScrollEvent, StyleObject, StyleProp } from './types';
