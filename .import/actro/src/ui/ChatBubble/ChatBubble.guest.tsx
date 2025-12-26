/**
 * ChatBubble - Guest Component
 */

import type { ChatBubbleProps } from '../../types';
import { createGuestComponent } from '../guest-factory';

export const ChatBubble = createGuestComponent<ChatBubbleProps>('ChatBubble');

export default ChatBubble;
