/**
 * AskIt - Host Entry
 *
 * This is the main entry point for React Native / Metro bundler.
 * Exports all UI components and APIs with host implementations.
 */

// UI Components
export { StepList, ThemeView, UserAvatar, ChatBubble } from './ui/index.host';

// APIs
export { EventEmitter, Toast, Haptic } from './api/index.host';

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

  // EventEmitter
  EventEmitterAPI,
  EventCallback,

  // Toast
  ToastAPI,
  ToastOptions,
  ToastPosition,
  ToastDuration,

  // Haptic
  HapticAPI,
  HapticType,
} from './types';
