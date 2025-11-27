/**
 * ChatBubble - Native Implementation (Host App)
 *
 * Chat message bubble with support for own/other messages,
 * timestamps, delivery status, and long press actions.
 */

import { useMemo } from 'react';
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  type ViewStyle,
  type TextStyle,
} from 'react-native';
import type { ChatBubbleProps } from '../../types';

// Theme colors
const COLORS = {
  ownBubble: '#007AFF',
  ownText: '#FFFFFF',
  otherBubble: '#E9E9EB',
  otherText: '#000000',
  timestamp: '#8E8E93',
  statusSending: '#C7C7CC',
  statusSent: '#8E8E93',
  statusDelivered: '#34C759',
  statusRead: '#007AFF',
  statusError: '#FF3B30',
};

/**
 * Format timestamp for display
 */
function formatTimestamp(timestamp?: string | number | Date): string {
  if (!timestamp) return '';

  const date = timestamp instanceof Date ? timestamp : new Date(timestamp);

  if (isNaN(date.getTime())) return '';

  const hours = date.getHours().toString().padStart(2, '0');
  const minutes = date.getMinutes().toString().padStart(2, '0');

  return `${hours}:${minutes}`;
}

/**
 * Get status icon
 */
function getStatusIcon(status?: ChatBubbleProps['status']): string {
  switch (status) {
    case 'sending':
      return '○';
    case 'sent':
      return '✓';
    case 'delivered':
      return '✓✓';
    case 'read':
      return '✓✓';
    case 'error':
      return '!';
    default:
      return '';
  }
}

/**
 * Get status color
 */
function getStatusColor(status?: ChatBubbleProps['status']): string {
  switch (status) {
    case 'sending':
      return COLORS.statusSending;
    case 'sent':
      return COLORS.statusSent;
    case 'delivered':
      return COLORS.statusDelivered;
    case 'read':
      return COLORS.statusRead;
    case 'error':
      return COLORS.statusError;
    default:
      return COLORS.timestamp;
  }
}

/**
 * ChatBubble Component
 */
export function ChatBubble({
  children,
  content,
  isOwn = false,
  showTail = true,
  timestamp,
  status,
  onLongPress,
  onPress,
  style,
}: ChatBubbleProps) {
  const bubbleStyle = useMemo(
    () => ({
      backgroundColor: isOwn ? COLORS.ownBubble : COLORS.otherBubble,
    }),
    [isOwn]
  );

  const textColor = isOwn ? COLORS.ownText : COLORS.otherText;
  const formattedTime = formatTimestamp(timestamp);
  const statusIcon = getStatusIcon(status);
  const statusColor = getStatusColor(status);

  const BubbleContent = (
    <View style={[styles.container, isOwn ? styles.ownContainer : styles.otherContainer]}>
      <View
        style={[
          styles.bubble,
          bubbleStyle,
          isOwn ? styles.ownBubble : styles.otherBubble,
          showTail && (isOwn ? styles.ownTail : styles.otherTail),
          style as ViewStyle,
        ]}
      >
        {/* Content */}
        {children || <Text style={[styles.content, { color: textColor }]}>{content}</Text>}

        {/* Footer: Timestamp and Status */}
        {(formattedTime || statusIcon) && (
          <View style={styles.footer}>
            {formattedTime && (
              <Text
                style={[
                  styles.timestamp,
                  { color: isOwn ? 'rgba(255,255,255,0.7)' : COLORS.timestamp },
                ]}
              >
                {formattedTime}
              </Text>
            )}
            {isOwn && statusIcon && (
              <Text style={[styles.status, { color: statusColor }]}>{statusIcon}</Text>
            )}
          </View>
        )}
      </View>
    </View>
  );

  if (onPress || onLongPress) {
    return (
      <TouchableOpacity
        onPress={onPress}
        onLongPress={onLongPress}
        activeOpacity={0.8}
        delayLongPress={300}
      >
        {BubbleContent}
      </TouchableOpacity>
    );
  }

  return BubbleContent;
}

const styles = StyleSheet.create({
  container: {
    flexDirection: 'row',
    marginVertical: 2,
    paddingHorizontal: 8,
  },
  ownContainer: {
    justifyContent: 'flex-end',
  },
  otherContainer: {
    justifyContent: 'flex-start',
  },
  bubble: {
    maxWidth: '80%',
    paddingHorizontal: 12,
    paddingVertical: 8,
    borderRadius: 18,
  },
  ownBubble: {
    borderBottomRightRadius: 4,
  },
  otherBubble: {
    borderBottomLeftRadius: 4,
  },
  ownTail: {
    borderBottomRightRadius: 4,
  },
  otherTail: {
    borderBottomLeftRadius: 4,
  },
  content: {
    fontSize: 16,
    lineHeight: 22,
  } as TextStyle,
  footer: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'flex-end',
    marginTop: 4,
    gap: 4,
  },
  timestamp: {
    fontSize: 11,
  } as TextStyle,
  status: {
    fontSize: 11,
    fontWeight: '600',
  } as TextStyle,
});

export default ChatBubble;
