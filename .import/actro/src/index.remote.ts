/**
 * AskIt - Remote Entry (Plugin/Sandbox)
 *
 * This is the main entry point for QuickJS / Rill sandbox environment.
 * Exports DSL identifiers that map to native components on Host.
 */

// UI Components (DSL identifiers)
export { StepList, ThemeView, UserAvatar, ChatBubble } from './ui/index.remote';

// APIs (Remote implementations)
export { Bus, Toast, Haptic } from './api/index.remote';

// Types (same as native for consistent API)
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
