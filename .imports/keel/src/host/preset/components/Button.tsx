/**
 * Button Component
 *
 * Default Button component implementation, wrapping React Native Button
 */

import type React from 'react';
import { Button as RNButton } from 'react-native';

export interface ButtonProps {
  title: string;
  onPress: () => void;
  color?: string;
  disabled?: boolean;
  testID?: string;
  accessibilityLabel?: string;
}

export function Button({
  title,
  onPress,
  color,
  disabled,
  testID,
  accessibilityLabel,
}: ButtonProps): React.ReactElement {
  // Wrap onPress to not pass the GestureResponderEvent to Guest
  // The event object contains native references that cannot be serialized across JSI boundary
  return (
    <RNButton
      title={title}
      onPress={() => onPress()}
      color={color}
      disabled={disabled}
      testID={testID}
      accessibilityLabel={accessibilityLabel}
    />
  );
}

export default Button;
