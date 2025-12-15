/**
 * ClickableView Component
 *
 * A clickable View component that works on macOS by using mouse events
 */

import React from 'react';
import { View, type ViewStyle } from 'react-native';

export interface ClickableViewProps {
  style?: ViewStyle;
  children?: React.ReactNode;
  onPress?: () => void;
  activeOpacity?: number;
  disabled?: boolean;
  testID?: string;
  accessible?: boolean;
  accessibilityLabel?: string;
  accessibilityRole?: 'button' | 'link' | 'none';
}

export function ClickableView({
  style,
  children,
  onPress,
  activeOpacity = 0.7,
  disabled = false,
  testID,
  accessible,
  accessibilityLabel,
  accessibilityRole = 'button',
}: ClickableViewProps): React.ReactElement {
  const [isPressed, setIsPressed] = React.useState(false);

  const handleMouseDown = () => {
    if (!disabled) {
      setIsPressed(true);
    }
  };

  const handleMouseUp = () => {
    if (!disabled) {
      setIsPressed(false);
      onPress?.();
    }
  };

  const handleMouseLeave = () => {
    setIsPressed(false);
  };

  const combinedStyle: ViewStyle = {
    ...style,
    opacity: isPressed ? activeOpacity : 1,
    cursor: disabled ? 'auto' : 'pointer',
  };

  const mouseHandlers = {
    onMouseDown: handleMouseDown,
    onMouseUp: handleMouseUp,
    onMouseLeave: handleMouseLeave,
  } as any;

  return (
    <View
      style={combinedStyle}
      {...mouseHandlers}
      testID={testID}
      accessible={accessible}
      accessibilityLabel={accessibilityLabel}
      accessibilityRole={accessibilityRole}
    >
      {children}
    </View>
  );
}

export default ClickableView;
