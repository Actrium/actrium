/**
 * ActivityIndicator Component
 *
 * Default ActivityIndicator component implementation, wrapping React Native ActivityIndicator
 */

import type React from 'react';
import { ActivityIndicator as RNActivityIndicator, type ViewStyle } from 'react-native';

export interface ActivityIndicatorProps {
  animating?: boolean;
  color?: string;
  size?: 'small' | 'large' | number;
  hidesWhenStopped?: boolean;
  style?: ViewStyle;
  testID?: string;
  accessible?: boolean;
  accessibilityLabel?: string;
}

export function ActivityIndicator({
  animating = true,
  color,
  size = 'small',
  hidesWhenStopped = true,
  style,
  testID,
  accessible,
  accessibilityLabel,
}: ActivityIndicatorProps): React.ReactElement {
  return (
    <RNActivityIndicator
      animating={animating}
      color={color}
      size={size}
      hidesWhenStopped={hidesWhenStopped}
      style={style}
      testID={testID}
      accessible={accessible}
      accessibilityLabel={accessibilityLabel}
    />
  );
}

export default ActivityIndicator;
