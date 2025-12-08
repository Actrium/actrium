/**
 * Rill SDK
 *
 * Virtual components and Hooks for plugin development
 * Zero runtime dependencies - all implementations injected by reconciler at runtime
 */

import type { StyleProp, LayoutEvent, ScrollEvent, ImageSource } from '../types';

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
  children?: React.ReactNode;
  onLayout?: (event: LayoutEvent) => void;
  pointerEvents?: 'auto' | 'none' | 'box-none' | 'box-only';
  accessible?: boolean;
  accessibilityLabel?: string;
}

/**
 * Text Component Props
 */
export interface TextProps extends BaseProps {
  children?: React.ReactNode;
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
  children?: React.ReactNode;
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
  renderItem: (info: { item: T; index: number }) => React.ReactNode;
  keyExtractor?: (item: T, index: number) => string;
  ItemSeparatorComponent?: React.ComponentType;
  ListHeaderComponent?: React.ReactNode;
  ListFooterComponent?: React.ReactNode;
  ListEmptyComponent?: React.ReactNode;
  numColumns?: number;
  initialNumToRender?: number;
  onEndReached?: () => void;
  onEndReachedThreshold?: number;
  refreshing?: boolean;
  onRefresh?: () => void;
  getItemLayout?: (data: T[] | null, index: number) => {
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
  keyboardType?:
    | 'default'
    | 'email-address'
    | 'numeric'
    | 'phone-pad'
    | 'decimal-pad'
    | 'url';
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
 * @param eventName Event name
 * @param callback Callback function
 *
 * @example
 * ```tsx
 * useHostEvent('REFRESH', () => {
 *   console.log('Host requested refresh');
 *   fetchData();
 * });
 * ```
 */
export function useHostEvent<T = unknown>(
  eventName: string,
  callback: (payload: T) => void
): void {
  // Actual implementation injected by reconciler at runtime
  // SDK only provides type signature
  const g = globalThis as Record<string, unknown>;
  if (typeof globalThis !== 'undefined' && '__useHostEvent' in globalThis) {
    (g['__useHostEvent'] as (name: string, cb: (payload: T) => void) => void)(eventName, callback);
  }
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
    return (g['__getConfig'] as () => T)();
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
    return g['__sendEventToHost'] as (eventName: string, payload?: unknown) => void;
  }
  return () => {
    console.warn('[rill] sendToHost is not available outside sandbox');
  };
}

// ============ Type Exports ============

export type { StyleProp, StyleObject, LayoutEvent, ScrollEvent, ImageSource } from '../types';
