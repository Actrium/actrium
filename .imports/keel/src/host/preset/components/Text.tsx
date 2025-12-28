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
  // Wrap callbacks to not pass GestureResponderEvent to Guest
  // The event object contains native references that cannot be serialized across JSI boundary
  return (
    <RNText
      style={style}
      numberOfLines={numberOfLines}
      ellipsizeMode={ellipsizeMode}
      selectable={selectable}
      testID={testID}
      accessible={accessible}
      accessibilityLabel={accessibilityLabel}
      onPress={onPress ? () => onPress() : undefined}
      onLongPress={onLongPress ? () => onLongPress() : undefined}
    >
      {children}
    </RNText>
  );
}

export default Text;
