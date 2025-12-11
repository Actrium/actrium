/**
 * askit - Guest Entry
 *
 * This is the main entry point for QuickJS / Rill sandbox environment.
 * Exports DSL identifiers that map to host components.
 */

// UI Components (DSL identifiers)
export { StepList, ThemeView, UserAvatar, ChatBubble } from './ui/index.guest';

// APIs (Guest implementations)
export { EventEmitter, Toast, Haptic } from './api/index.guest';

// Types (same as host for consistent API)
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
