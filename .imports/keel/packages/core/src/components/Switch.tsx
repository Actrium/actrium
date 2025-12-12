/**
 * Switch Component
 *
 * Default Switch component implementation, wrapping React Native Switch
 */

import type React from 'react';
import { Switch as RNSwitch } from 'react-native';

export interface SwitchProps {
  value?: boolean;
  onValueChange?: (value: boolean) => void;
  disabled?: boolean;
  trackColor?: { false?: string; true?: string };
  thumbColor?: string;
  ios_backgroundColor?: string;
  testID?: string;
  accessible?: boolean;
  accessibilityLabel?: string;
}

export function Switch({
  value,
  onValueChange,
  disabled,
  trackColor,
  thumbColor,
  ios_backgroundColor,
  testID,
  accessible,
  accessibilityLabel,
}: SwitchProps): React.ReactElement {
  return (
    <RNSwitch
      value={value}
      onValueChange={onValueChange}
      disabled={disabled}
      trackColor={trackColor}
      thumbColor={thumbColor}
      ios_backgroundColor={ios_backgroundColor}
      testID={testID}
      accessible={accessible}
      accessibilityLabel={accessibilityLabel}
    />
  );
}

export default Switch;
