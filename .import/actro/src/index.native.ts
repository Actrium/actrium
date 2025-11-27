/**
 * AskIt - Native Entry (Host App)
 *
 * This is the main entry point for React Native / Metro bundler.
 * Exports all UI components and APIs with native implementations.
 */

// UI Components
export { StepList, ThemeView, UserAvatar, ChatBubble } from './ui/index.native';

// APIs
export { Bus, Toast, Haptic } from './api/index.native';

// Types
export type {
  // Common
  ViewStyle,
  TextStyle,
  ImageStyle,
  BaseProps,
  Environment,

  // StepList
  StepListProps,
  StepItem,
  StepStatus,

  // ThemeView
  ThemeViewProps,

  // UserAvatar
  UserAvatarProps,

  // ChatBubble
  ChatBubbleProps,

  // Bus
  BusAPI,
  BusEventCallback,

  // Toast
  ToastAPI,
  ToastOptions,
  ToastPosition,
  ToastDuration,

  // Haptic
  HapticAPI,
  HapticType,
} from './types';
