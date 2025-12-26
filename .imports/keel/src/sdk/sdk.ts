/**
 * Rill SDK
 *
 * Virtual components and Hooks for Guest development
 * Zero runtime dependencies - all implementations injected by reconciler at runtime
 */

import type {
  ImageSource,
  LayoutEvent,
  RemoteRef,
  RemoteRefCallback,
  ScrollEvent,
  StyleProp,
} from './types';

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
  useEffect: (effect: () => undefined | (() => void), deps?: unknown[]) => void;
  useRef: <T>(initial: T) => { current: T };
  useState: <T>(initial: T | (() => T)) => [T, (value: T | ((prev: T) => T)) => void];
  useMemo: <T>(factory: () => T, deps: unknown[]) => T;
  useCallback: <T extends (...args: unknown[]) => unknown>(callback: T, deps: unknown[]) => T;
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
  const throwUnavailable = (hook: string) => () => {
    throw new Error(`[rill/sdk] ${hook} not available. Ensure running in rill sandbox.`);
  };
  return {
    useEffect: throwUnavailable('useEffect'),
    useRef: throwUnavailable('useRef'),
    useState: throwUnavailable('useState'),
    useMemo: throwUnavailable('useMemo'),
    useCallback: throwUnavailable('useCallback'),
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
    return undefined;
  });

  useEffect(() => {
    const g = globalThis as Record<string, unknown>;
    if ('__useHostEvent' in globalThis) {
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
  if ('__getConfig' in globalThis) {
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
  if ('__sendEventToHost' in globalThis) {
    return g.__sendEventToHost as (eventName: string, payload?: unknown) => void;
  }
  return () => {
    console.warn('[rill] sendToHost is not available outside sandbox');
  };
}

// ============ Remote Ref ============

/** Pending call entry for tracking async method invocations */
interface PendingCall {
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
  timeoutId: ReturnType<typeof setTimeout>;
}

/** REF_METHOD_RESULT message from Host */
interface RefMethodResult {
  refId: number;
  callId: string;
  result?: unknown;
  error?: { message: string; name?: string; stack?: string };
}

/** Default timeout for remote method calls (ms) */
const DEFAULT_REMOTE_REF_TIMEOUT = 4000;

/**
 * Create a remote reference to a Host component instance
 *
 * Since Guest code runs in a sandbox and cannot directly access Host component instances,
 * this hook provides a message-based mechanism to invoke component methods asynchronously.
 *
 * @returns Tuple of [refCallback, remoteRef]
 *   - refCallback: Pass to component's ref prop
 *   - remoteRef: RemoteRef instance (null until mounted)
 *
 * @example
 * ```tsx
 * import { useRemoteRef, TextInput, TextInputRef } from 'rill/sdk';
 *
 * function MyComponent() {
 *   const [inputRef, remoteInput] = useRemoteRef<TextInputRef>();
 *
 *   const handleFocus = async () => {
 *     await remoteInput?.invoke('focus');
 *     // or using typed call proxy:
 *     // await remoteInput?.call.focus();
 *   };
 *
 *   return (
 *     <TouchableOpacity onPress={handleFocus}>
 *       <TextInput ref={inputRef} placeholder="Tap button to focus" />
 *     </TouchableOpacity>
 *   );
 * }
 * ```
 */
export function useRemoteRef<T = unknown>(options?: {
  timeout?: number;
}): [RemoteRefCallback, RemoteRef<T> | null] {
  const { useEffect, useRef, useState, useMemo, useCallback } = getReactHooks();

  // Use provided timeout or default
  const timeout = options?.timeout ?? DEFAULT_REMOTE_REF_TIMEOUT;

  // Track the nodeId assigned by reconciler
  const [nodeId, setNodeId] = useState<number | null>(null);

  // Counter for generating unique call IDs
  const callIdCounterRef = useRef(0);

  // Map of pending calls waiting for results
  const pendingCallsRef = useRef<Map<string, PendingCall>>(new Map());

  // Ref callback to pass to the component
  const refCallback = useCallback(
    ((instance: { nodeId: number } | null) => {
      if (instance && typeof instance.nodeId === 'number') {
        setNodeId(instance.nodeId);
      } else {
        setNodeId(null);
      }
    }) as unknown as (...args: unknown[]) => unknown,
    []
  ) as unknown as RemoteRefCallback;

  // Listen for REF_METHOD_RESULT events from Host
  useEffect(() => {
    const g = globalThis as Record<string, unknown>;
    if (!('__useHostEvent' in g)) {
      return undefined;
    }

    const handleResult = (message: RefMethodResult) => {
      // Only handle results for our nodeId
      if (message.refId !== nodeId) {
        return;
      }

      const pending = pendingCallsRef.current.get(message.callId);
      if (!pending) {
        return;
      }

      // Clear timeout and remove from pending
      clearTimeout(pending.timeoutId);
      pendingCallsRef.current.delete(message.callId);

      // Resolve or reject the promise
      if (message.error) {
        const error = new Error(message.error.message);
        if (message.error.name) error.name = message.error.name;
        if (message.error.stack) error.stack = message.error.stack;
        pending.reject(error);
      } else {
        pending.resolve(message.result);
      }
    };

    // Subscribe to __REF_RESULT__ events
    const unsubscribe = (
      g.__useHostEvent as (name: string, cb: (payload: RefMethodResult) => void) => () => void
    )('__REF_RESULT__', handleResult);

    return unsubscribe;
  }, [nodeId]);

  // Cleanup pending calls when nodeId changes or on unmount
  useEffect(() => {
    return () => {
      // Reject all pending calls - node may have changed or unmounted
      for (const [, pending] of pendingCallsRef.current) {
        clearTimeout(pending.timeoutId);
        pending.reject(new Error('Node changed or component unmounted'));
      }
      pendingCallsRef.current.clear();
    };
  }, [nodeId]);

  // Create RemoteRef instance
  const remoteRef = useMemo<RemoteRef<T> | null>(() => {
    if (nodeId === null) {
      return null;
    }

    const invoke = <R = unknown>(method: string, ...args: unknown[]): Promise<R> => {
      return new Promise((resolve, reject) => {
        // Generate unique call ID
        const callId = `${nodeId}-${++callIdCounterRef.current}`;

        // Set timeout for the call
        const timeoutId = setTimeout(() => {
          pendingCallsRef.current.delete(callId);
          reject(new Error(`Remote method call '${method}' timed out after ${timeout}ms`));
        }, timeout);

        // Store pending call
        pendingCallsRef.current.set(callId, {
          resolve: resolve as (value: unknown) => void,
          reject,
          timeoutId,
        });

        // Send REF_CALL operation to Host
        const g = globalThis as Record<string, unknown>;
        if ('__sendOperation' in g) {
          const sendOp = g.__sendOperation as (op: unknown) => void;
          sendOp({
            op: 'REF_CALL',
            refId: nodeId,
            method,
            args,
            callId,
          });
        } else {
          // No operation channel available
          clearTimeout(timeoutId);
          pendingCallsRef.current.delete(callId);
          reject(new Error('[rill/sdk] __sendOperation not available'));
        }
      });
    };

    // Create typed call proxy
    const call = new Proxy(
      {},
      {
        get(_, prop: string) {
          return (...args: unknown[]) => invoke(prop, ...args);
        },
      }
    ) as RemoteRef<T>['call'];

    return {
      nodeId,
      invoke,
      call,
    };
  }, [nodeId, timeout]);

  return [refCallback, remoteRef];
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
  globalThis.React && typeof (globalThis.React as { Component?: unknown }).Component === 'function'
    ? globalThis.React
    : { Component: class {} };

type ReactType = typeof React;
// Reason: ReactNode fallback type when React.Component unavailable
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
      // Reason: Error payload can be any serializable type
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
