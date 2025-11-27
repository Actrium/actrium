/**
 * ThemeView - Native Implementation (Host App)
 *
 * A themed container view with default background and padding.
 * Supports different variants for common UI patterns.
 */

import React from 'react';
import { View, StyleSheet, type ViewStyle } from 'react-native';
import type { ThemeViewProps } from '../../types';

// Theme colors (can be extended to support dark mode)
const THEME_COLORS = {
  primary: '#007AFF',
  secondary: '#5856D6',
  surface: '#FFFFFF',
  background: '#F2F2F7',
};

// Padding presets
const PADDING_MAP: Record<string, number> = {
  none: 0,
  small: 8,
  medium: 16,
  large: 24,
};

/**
 * Get background color for variant
 */
function getBackgroundColor(variant?: ThemeViewProps['variant']): string {
  switch (variant) {
    case 'primary':
      return THEME_COLORS.primary;
    case 'secondary':
      return THEME_COLORS.secondary;
    case 'surface':
      return THEME_COLORS.surface;
    case 'background':
    default:
      return THEME_COLORS.background;
  }
}

/**
 * Get padding value
 */
function getPadding(padding?: ThemeViewProps['padding']): number {
  if (typeof padding === 'number') {
    return padding;
  }
  if (typeof padding === 'string') {
    return PADDING_MAP[padding] ?? PADDING_MAP.medium;
  }
  return PADDING_MAP.medium;
}

/**
 * ThemeView Component
 */
export function ThemeView({
  children,
  variant = 'background',
  padding = 'medium',
  style,
}: ThemeViewProps) {
  const backgroundColor = getBackgroundColor(variant);
  const paddingValue = getPadding(padding);

  return (
    <View
      style={[styles.container, { backgroundColor, padding: paddingValue }, style as ViewStyle]}
    >
      {children}
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
});

export default ThemeView;
