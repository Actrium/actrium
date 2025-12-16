/**
 * Text Component
 *
 * Default Text component implementation, wrapping React Native Text
 */

import type React from 'react';
import { Text as RNText, type TextStyle } from 'react-native';

export interface TextProps {
  style?: TextStyle;
  children?: React.ReactNode;
  numberOfLines?: number;
  ellipsizeMode?: 'head' | 'middle' | 'tail' | 'clip';
  selectable?: boolean;
  testID?: string;
  accessible?: boolean;
  accessibilityLabel?: string;
  onPress?: () => void;
  onLongPress?: () => void;
}

export function Text({
  style,
  children,
  numberOfLines,
  ellipsizeMode,
  selectable,
  testID,
  accessible,
  accessibilityLabel,
  onPress,
  onLongPress,
}: TextProps): React.ReactElement {
  return (
    <RNText
      style={style}
      numberOfLines={numberOfLines}
      ellipsizeMode={ellipsizeMode}
      selectable={selectable}
      testID={testID}
      accessible={accessible}
      accessibilityLabel={accessibilityLabel}
      onPress={onPress}
      onLongPress={onLongPress}
    >
      {children}
    </RNText>
  );
}

export default Text;
