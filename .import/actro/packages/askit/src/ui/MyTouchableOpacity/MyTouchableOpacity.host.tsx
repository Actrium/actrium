/**
 * MyTouchableOpacity - Host Implementation
 *
 * Wraps TouchableOpacity with a control parameter to enable/disable touch interactions.
 */

import { TouchableOpacity, type ViewStyle, type GestureResponderEvent } from 'react-native';
import type { MyTouchableOpacityProps } from '../../types';

const getSerializableEvent = (event: GestureResponderEvent) => {
  // 我们只关心 nativeEvent
  const { nativeEvent } = event;

  if (!nativeEvent) return null;

  return {
    __custom: true,
    target: nativeEvent.target,
    locationX: nativeEvent.locationX,
    locationY: nativeEvent.locationY,
    pageX: nativeEvent.pageX,
    pageY: nativeEvent.pageY,
    timestamp: nativeEvent.timestamp,
    identifier: nativeEvent.identifier,
    // 如果是多指触控，可以选填提取 touches 数组
    touches: nativeEvent.touches?.map((t: any) => ({
      locationX: t.locationX,
      locationY: t.locationY,
      pageX: t.pageX,
      pageY: t.pageY,
    })),
  };
};

/**
 * MyTouchableOpacity Component
 */
export function MyTouchableOpacity({
  children,
  onPress,
  onPressIn,
  onPressOut,
  onLongPress,
  activeOpacity,
  disabled = false,
  delayPressIn,
  delayPressOut,
  delayLongPress,
  style,
  testID,
  accessible,
  accessibilityLabel,
  accessibilityRole = 'button',
}: MyTouchableOpacityProps) {
  return (
    <TouchableOpacity
      onPress={(event: GestureResponderEvent) => {
        const customEvent = getSerializableEvent(event);

        // event.preventDefault()
        // event.stopPropagation()

        onPress?.(customEvent as unknown as GestureResponderEvent);
      }}
      onPressIn={(event: GestureResponderEvent) => onPressIn?.(event)}
      onPressOut={(event: GestureResponderEvent) => onPressOut?.(event)}
      onLongPress={(event: GestureResponderEvent) => onLongPress?.(event)}
      activeOpacity={activeOpacity}
      disabled={disabled}
      delayPressIn={delayPressIn}
      delayPressOut={delayPressOut}
      delayLongPress={delayLongPress}
      style={style as ViewStyle}
      testID={testID}
      accessible={accessible}
      accessibilityLabel={accessibilityLabel}
      accessibilityRole={accessibilityRole}
    >
      {children}
    </TouchableOpacity>
  );
}

export default MyTouchableOpacity;
