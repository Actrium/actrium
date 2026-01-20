/**
 * AskIt - Host Entry
 *
 * This is the main entry point for React Native / Metro bundler.
 * Exports all UI components and APIs with host implementations.
 */

// APIs
export { EventEmitter, Haptic, Toast } from './api/index.host';
// Types
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
  StyleObject,
  StyleProp,
  StepItem,
  // StepList
  StepListProps,
  StepStatus,
  // ThemeView
  ThemeViewProps,
  // Toast
  ToastAPI,
  ToastDuration,
  ToastOptions,
  ToastPosition,
  // UserAvatar
  UserAvatarProps,
  MyTouchableOpacityProps,
} from './types';
// UI Components
export {
  ChatBubble,
  EngineMonitorOverlay,
  PanelMarker,
  StepList,
  ThemeView,
  UserAvatar,
  MyTouchableOpacity,
   DropdownMenu,
  DropdownMenuItem,
  DropdownMenuText,
  DropdownMenuSubText,
} from './ui/index.host';

// Panel utilities (Host-only)
export { extractPanels } from './ui/Panel/extractPanels.host';
