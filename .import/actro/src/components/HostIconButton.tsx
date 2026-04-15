import React from 'react';
import {
  Animated,
  Insets,
  ImageStyle,
  Platform,
  StyleProp,
  StyleSheet,
  TouchableOpacity,
  ViewStyle,
} from 'react-native';
import { Icon } from './Icon';
import { hostIconButtonSize, hostIconGlyphSize } from '../theme/tokens';
import { useColors } from '../theme/useColors';

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
    size = hostIconButtonSize.md,
    borderRadius = size / 2,
    iconSize = hostIconGlyphSize.sm,
    iconColor,
    activeIconColor,
    backgroundColor,
    activeBackgroundColor,
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
    const colors = useColors();
    const finalIconColor = iconColor ?? colors.iconHostIconButton;
    const finalActiveIconColor = activeIconColor ?? colors.iconHostIconButtonActive;
    const finalBackgroundColor = backgroundColor ?? colors.bgHostIconButton;
    const finalActiveBackgroundColor = activeBackgroundColor ?? colors.bgHostIconButtonActive;

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

    const containerTransform =
      Platform.OS === 'android' ? undefined : [{ scale }];

    return (
      <Animated.View
        style={[wrapperStyle, { opacity, transform: containerTransform }]}
      >
        <TouchableOpacity
          activeOpacity={activeOpacity}
          style={[
            styles.button,
            {
              width: size,
              height: size,
              borderRadius,
              backgroundColor: active ? finalActiveBackgroundColor : finalBackgroundColor,
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
          <Animated.View>
            {iconType ? (
              <Icon
                type={iconType}
                size={iconSize}
                color={active ? finalActiveIconColor : finalIconColor}
                style={iconStyle}
                disableTint={disableTint}
              />
            ) : null}
            {children}
          </Animated.View>
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
