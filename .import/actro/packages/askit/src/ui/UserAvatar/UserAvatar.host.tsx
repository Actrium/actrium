/**
 * UserAvatar - Host Implementation
 *
 * Displays user avatar with optional online status indicator.
 * Supports different sizes and fallback to initials.
 */

import { useMemo, useState } from 'react';
import {
  Image,
  StyleSheet,
  Text,
  type TextStyle,
  TouchableOpacity,
  View,
  type ViewStyle,
} from 'react-native';
import type { UserAvatarProps } from '../../types';

// Size presets
const SIZE_MAP: Record<string, number> = {
  small: 32,
  medium: 48,
  large: 64,
};

// Background colors for initials (based on name hash)
const AVATAR_COLORS = [
  '#FF6B6B',
  '#4ECDC4',
  '#45B7D1',
  '#96CEB4',
  '#FFEAA7',
  '#DDA0DD',
  '#98D8C8',
  '#F7DC6F',
  '#BB8FCE',
  '#85C1E9',
];

/**
 * Get initials from name
 */
function getInitials(name?: string): string {
  if (!name) return '?';
  const parts = name.trim().split(/\s+/);
  if (parts.length === 1) {
    return parts[0]?.charAt(0).toUpperCase() ?? '?';
  }
  const first = parts[0]?.charAt(0) ?? '';
  const last = parts[parts.length - 1]?.charAt(0) ?? '';
  return (first + last).toUpperCase();
}

/**
 * Get background color from name (consistent hash)
 */
function getColorFromName(name?: string): string {
  if (!name) return AVATAR_COLORS[0] ?? '#FF6B6B';
  let hash = 0;
  for (let i = 0; i < name.length; i++) {
    hash = name.charCodeAt(i) + ((hash << 5) - hash);
  }
  return AVATAR_COLORS[Math.abs(hash) % AVATAR_COLORS.length] ?? '#FF6B6B';
}

/**
 * Get numeric size value
 */
function getSize(size?: UserAvatarProps['size']): number {
  if (typeof size === 'number') {
    return size;
  }
  return SIZE_MAP[size || 'medium'] ?? SIZE_MAP['medium'] ?? 48;
}

/**
 * UserAvatar Component
 */
export function UserAvatar({
  uri,
  name,
  size = 'medium',
  showOnlineStatus = false,
  isOnline = false,
  onPress,
  style,
}: UserAvatarProps) {
  const [imageError, setImageError] = useState(false);
  const numericSize = getSize(size);

  const initials = useMemo(() => getInitials(name), [name]);
  const bgColor = useMemo(() => getColorFromName(name), [name]);

  const showImage = uri && !imageError;
  const statusSize = Math.max(numericSize * 0.25, 8);

  const containerStyle: ViewStyle = {
    width: numericSize,
    height: numericSize,
    borderRadius: numericSize / 2,
  };

  const Content = (
    <View style={[styles.container, containerStyle, style as ViewStyle]}>
      {showImage ? (
        <Image
          source={{ uri }}
          style={[styles.image, containerStyle as object]}
          onError={() => setImageError(true)}
        />
      ) : (
        <View style={[styles.initialsContainer, containerStyle, { backgroundColor: bgColor }]}>
          <Text style={[styles.initials, { fontSize: numericSize * 0.4 }]}>{initials}</Text>
        </View>
      )}

      {showOnlineStatus && (
        <View
          style={[
            styles.statusDot,
            {
              width: statusSize,
              height: statusSize,
              borderRadius: statusSize / 2,
              backgroundColor: isOnline ? '#34C759' : '#8E8E93',
              right: 0,
              bottom: 0,
              borderWidth: Math.max(statusSize * 0.2, 1.5),
            },
          ]}
        />
      )}
    </View>
  );

  if (onPress) {
    return (
      <TouchableOpacity onPress={onPress} activeOpacity={0.7}>
        {Content}
      </TouchableOpacity>
    );
  }

  return Content;
}

const styles = StyleSheet.create({
  container: {
    position: 'relative',
    overflow: 'visible',
  },
  image: {
    resizeMode: 'cover',
  },
  initialsContainer: {
    alignItems: 'center',
    justifyContent: 'center',
  },
  initials: {
    color: '#FFFFFF',
    fontWeight: '600',
  } as TextStyle,
  statusDot: {
    position: 'absolute',
    borderColor: '#FFFFFF',
  },
});

export default UserAvatar;
