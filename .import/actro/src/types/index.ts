/**
 * AskIt Types - Shared type definitions for Host and Guest
 */

import type { ReactNode } from 'react';

// ============================================================================
// Environment Detection
// ============================================================================

export type Environment = 'native' | 'remote';

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
// Bus (Communication) Types
// ============================================================================

export type BusEventCallback<T = unknown> = (payload: T) => void;

export interface BusAPI {
  emit: (event: string, payload?: unknown) => void;
  on: <T = unknown>(event: string, callback: BusEventCallback<T>) => void;
  off: <T = unknown>(event: string, callback: BusEventCallback<T>) => void;
  once: <T = unknown>(event: string, callback: BusEventCallback<T>) => void;
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
