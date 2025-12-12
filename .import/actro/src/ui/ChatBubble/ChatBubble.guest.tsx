/**
 * ChatBubble - Guest Component (Type-Safe)
 *
 * Displays a chat message bubble with optional metadata
 */

import { createGuestComponent } from '../../core/component-helpers';
import type { ChatBubbleProps } from '../../types';

/**
 * ChatBubble component factory
 *
 * @example
 * ```tsx
 * import { ChatBubble } from 'askit';
 *
 * const bubble = ChatBubble({
 *   content: 'Hello, world!',
 *   isOwn: true,
 *   showTail: true,
 *   timestamp: new Date(),
 *   status: 'delivered',
 * });
 * ```
 */
export const ChatBubble = createGuestComponent<ChatBubbleProps>('ChatBubble');

export default ChatBubble;
