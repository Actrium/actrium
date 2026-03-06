import React from 'react';
import {
  Animated,
  Insets,
  ImageStyle,
  StyleProp,
  StyleSheet,
  TouchableOpacity,
  ViewStyle,
} from 'react-native';
import { Icon } from './Icon';

interface HostIconButtonProps {
  iconType?: string;
  onPress?: () => void;
  disabled?: boolean;
  active?: boolean;
  size?: number;
  borderRadius?: number;
  iconSize?: number;
  iconColor?: string;
  activeIconColor?: string;
  backgroundColor?: string;
  activeBackgroundColor?: string;
  wrapperStyle?: StyleProp<ViewStyle>;
  style?: StyleProp<ViewStyle>;
  iconStyle?: StyleProp<ImageStyle>;
  disableTint?: boolean;
  activeOpacity?: number;
  pressScale?: number;
  pressedOpacity?: number;
  enablePressScale?: boolean;
  hitSlop?: Insets | number;
  children?: React.ReactNode;
}

export const HostIconButton: React.FC<HostIconButtonProps> = React.memo(
  ({
    iconType,
    onPress,
    disabled = false,
    active = false,
    size = 44,
    borderRadius = size / 2,
    iconSize = 16,
    iconColor = '#0a0a0a',
    activeIconColor = '#FFFFFF',
    backgroundColor = '#DFDFE1',
    activeBackgroundColor = '#0088FF',
    wrapperStyle,
    style,
    iconStyle,
    disableTint = false,
    activeOpacity = 1,
    pressScale = 1.34,
    pressedOpacity = 0.58,
    enablePressScale = true,
    hitSlop,
    children,
  }) => {
    const scale = React.useRef(new Animated.Value(1)).current;
    const opacity = React.useRef(new Animated.Value(1)).current;

    const animatePressState = React.useCallback(
      (nextPressed: boolean) => {
        Animated.parallel([
          Animated.spring(scale, {
            toValue: nextPressed && enablePressScale ? pressScale : 1,
            useNativeDriver: true,
            speed: 28,
            bounciness: 5,
          }),
          Animated.timing(opacity, {
            toValue: nextPressed ? pressedOpacity : 1,
            duration: nextPressed ? 120 : 180,
            useNativeDriver: true,
          }),
        ]).start();
      },
      [enablePressScale, opacity, pressScale, pressedOpacity, scale],
    );

    return (
      <Animated.View style={[wrapperStyle, { opacity, transform: [{ scale }] }]}>
        <TouchableOpacity
          activeOpacity={activeOpacity}
          style={[
            styles.button,
            {
              width: size,
              height: size,
              borderRadius,
              backgroundColor: active ? activeBackgroundColor : backgroundColor,
            },
            disabled && styles.disabled,
            style,
          ]}
          onPress={onPress}
          hitSlop={hitSlop}
          onPressIn={() => {
            if (!disabled) {
              animatePressState(true);
            }
          }}
          onPressOut={() => {
            animatePressState(false);
          }}
          disabled={disabled}
        >
          {iconType ? (
            <Icon
              type={iconType}
              size={iconSize}
              color={active ? activeIconColor : iconColor}
              style={iconStyle}
              disableTint={disableTint}
            />
          ) : null}
          {children}
        </TouchableOpacity>
      </Animated.View>
    );
  },
);

const styles = StyleSheet.create({
  button: {
    alignItems: 'center',
    justifyContent: 'center',
  },
  disabled: {
    opacity: 0.5,
  },
});
