import React, { useState, useRef } from 'react';
import {
  View,
  StyleSheet,
  Pressable,
  Modal,
  TouchableWithoutFeedback,
  Dimensions,
  GestureResponderEvent,
} from 'react-native';
import { AppText } from '../../../components';
import {
  getTypographyStyle,
  longPressMenu,
  spacing,
} from '../../../theme/tokens';
import { HOST_SHADOWS } from '../../styles/shadows';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import ReactNativeHapticFeedback from 'react-native-haptic-feedback'

interface MenuOption {
  label: string;
  onPress: (position?: { top: number; left: number; width: number }) => void;
  icon?: React.ReactNode;
  textColor?: string;
}

interface LongPressMenuProps {
  children: React.ReactNode;
  options: MenuOption[];
  disabled?: boolean;
  onPress?: () => void;
}

export const LongPressMenu: React.FC<LongPressMenuProps> = ({
  children,
  options,
  disabled,
  onPress,
}) => {
  const colors = useColors();
  const insets = useSafeAreaInsets();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);
  const [visible, setVisible] = useState(false);
  const [pressLocation, setPressLocation] = useState<{ x: number; y: number } | null>(null);
  const [menuSize, setMenuSize] = useState({ width: longPressMenu.width, height: 0 });
  const triggerRef = useRef<View>(null);
  const pressLocationRef = useRef<{ x: number; y: number } | null>(null);

  const handlePressIn = (event: GestureResponderEvent) => {
    if (disabled) return;

    // 中文注释：优先记录手指落点，后续菜单以这个锚点定位，而不是依赖整个气泡的位置。
    const nativeEvent = event?.nativeEvent;
    if (nativeEvent) {
      if (nativeEvent.pageX !== undefined && nativeEvent.pageY !== undefined) {
        pressLocationRef.current = {
          x: nativeEvent.pageX,
          y: nativeEvent.pageY,
        };
      } else if (nativeEvent.locationX !== undefined && nativeEvent.locationY !== undefined) {
        // 如果是相对位置，需要转换为绝对位置
        triggerRef.current?.measureInWindow((x, y) => {
          pressLocationRef.current = {
            x: x + nativeEvent.locationX,
            y: y + nativeEvent.locationY,
          };
        });
      }
    }
  };

  const handleLongPress = () => {
    if (disabled) return;

    ReactNativeHapticFeedback.trigger('impactHeavy')

    triggerRef.current?.measureInWindow((x, y, width, height) => {
      setPressLocation(
        pressLocationRef.current ?? {
          x: x + width / 2,
          y: y + height / 2,
        },
      );
      setMenuSize((prev) => ({ ...prev, height: 0 }));
      setVisible(true);
    });
  };

  const close = () => {
    setVisible(false);
    setPressLocation(null);
    pressLocationRef.current = null;
  };

  const renderMenu = () => {
    if (!pressLocation) return null;

    const screenHeight = Dimensions.get('window').height;
    const screenWidth = Dimensions.get('window').width;
    const horizontalMargin = 20;
    const verticalMargin = 20;
    const anchorOffset = 8;
    const topBoundary = Math.max(insets.top + verticalMargin, verticalMargin);
    const bottomBoundary = screenHeight - Math.max(insets.bottom + verticalMargin, verticalMargin);

    const menuWidth = longPressMenu.width;
    const estimatedMenuHeight =
      longPressMenu.padding * 2 + options.length * (spacing.md * 2 + 24) + Math.max(0, options.length - 1) * spacing.xxs;
    const menuHeight = menuSize.height || estimatedMenuHeight;

    // 中文注释：菜单优先贴近手指下方；下方空间不足时切到上方，并在最终结果上做屏幕边界钳制。
    const belowTop = pressLocation.y + anchorOffset;
    const aboveTop = pressLocation.y - menuHeight - anchorOffset;
    const canFitBelow = belowTop + menuHeight <= bottomBoundary;
    const canFitAbove = aboveTop >= topBoundary;
    const preferredTop = canFitBelow || !canFitAbove ? belowTop : aboveTop;
    const maxTop = Math.max(topBoundary, bottomBoundary - menuHeight);
    const top = Math.min(Math.max(preferredTop, topBoundary), maxTop);

    // 中文注释：横向仍以手指位置居中，但必须保证整个菜单留在屏幕可见区域内。
    const left = Math.min(
      Math.max(pressLocation.x - menuWidth / 2, horizontalMargin),
      screenWidth - horizontalMargin - menuWidth,
    );
    const menuOpacity = menuSize.height > 0 ? 1 : 0;

    return (
      <Modal
        visible={visible}
        transparent
        animationType='fade'
        onRequestClose={close}
      >
        <TouchableWithoutFeedback onPress={close}>
          <View style={styles.modalBackdrop}>
            <View
              style={[
                styles.menuContainer,
                {
                  top,
                  left,
                  width: menuWidth,
                  opacity: menuOpacity,
                },
              ]}
              onLayout={(event) => {
                const { width, height } = event.nativeEvent.layout;
                if (
                  Math.abs(width - menuSize.width) > 1 ||
                  Math.abs(height - menuSize.height) > 1
                ) {
                  setMenuSize({ width, height });
                }
              }}
            >
              {options.map((option, index) => (
                <Pressable
                  key={index}
                  style={({ pressed }) => [
                    styles.menuItem,
                    pressed && styles.menuItemPressed,
                  ]}
                  onPress={() => {
                    option.onPress({
                      top,
                      left,
                      width: menuWidth,
                    });
                    close();
                  }}
                >
                  {option.icon}
                  <AppText
                    variant='menuLabel'
                    style={[
                      styles.menuText,
                      option.textColor && { color: option.textColor },
                    ]}
                  >
                    {option.label}
                  </AppText>
                </Pressable>
              ))}
            </View>
          </View>
        </TouchableWithoutFeedback>
      </Modal>
    );
  };

  return (
    <>
      <View ref={triggerRef} collapsable={false}>
        <Pressable
          onPress={() => {
            if (disabled) return;
            onPress?.();
          }}
          onPressIn={handlePressIn}
          onLongPress={handleLongPress}
          delayLongPress={400}
          unstable_pressDelay={100}
          style={({ pressed }) => ({
            opacity: pressed ? 0.8 : 1,
          })}
        >
          {children}
        </Pressable>
      </View>
      {renderMenu()}
    </>
  );
};

const makeStyles = (colors: ColorTokens) => StyleSheet.create({
  modalBackdrop: {
    flex: 1,
    backgroundColor: 'rgba(0,0,0,0.1)',
  },
  menuContainer: {
    position: 'absolute',
    backgroundColor: colors.bgSurface,
    borderRadius: 34,
    padding: longPressMenu.padding,
    gap: spacing.xxs,
    ...HOST_SHADOWS.menuSurface,
  },
  menuItem: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingVertical: spacing.md,
    paddingHorizontal: spacing.lg,
    borderRadius: 34,
    gap: spacing.md,
  },
  menuItemPressed: {
    backgroundColor: colors.bgMenuItemPressed,
  },
  menuText: {
    ...getTypographyStyle('longPressMenuLabel'),
    color: colors.textMenuLabel,
    fontWeight: '500',
  },
});
