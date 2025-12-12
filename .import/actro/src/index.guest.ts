/**
 * askit - Guest Entry
 *
 * This is the main entry point for QuickJS / Rill sandbox environment.
 * Exports DSL identifiers that map to host components.
 */

// APIs (Guest implementations)
export { EventEmitter, Haptic, Toast } from './api/index.guest';
// Types (same as host for consistent API)
export type {
  BaseProps,
  // ChatBubble
  ChatBubbleProps,
  Environment,
  EventCallback,
  // EventEmitter
  EventEmitterAPI,
  // Haptic
  HapticAPI,
  HapticType,
  ImageStyle,
  StepItem,
  // StepList
  StepListProps,
  StepStatus,
  TextStyle,
  // ThemeView
  ThemeViewProps,
  // Toast
  ToastAPI,
  ToastDuration,
  ToastOptions,
  ToastPosition,
  // UserAvatar
  UserAvatarProps,
  // Common
  ViewStyle,
} from './types';
// UI Components (DSL identifiers)
export { ChatBubble, StepList, ThemeView, UserAvatar } from './ui/index.guest';
