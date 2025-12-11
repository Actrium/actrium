/**
 * UI Components - Guest exports
 *
 * These are DSL identifiers that map to host components.
 */

export { StepList } from './StepList/StepList.guest';
export { ThemeView } from './ThemeView/ThemeView.guest';
export { UserAvatar } from './UserAvatar/UserAvatar.guest';
export { ChatBubble } from './ChatBubble/ChatBubble.guest';

// Re-export types (same types for both environments)
export type {
  StepListProps,
  StepItem,
  StepStatus,
  ThemeViewProps,
  UserAvatarProps,
  ChatBubbleProps,
} from '../types';
