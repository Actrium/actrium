import React, { useState, useRef, useCallback, useEffect } from 'react';
import {
  View,
  StyleSheet,
  Pressable,
  Modal,
  TouchableWithoutFeedback,
  Dimensions,
  GestureResponderEvent,
} from 'react-native';
import { Gesture, GestureDetector } from 'react-native-gesture-handler';
import Animated, {
  useSharedValue,
  useAnimatedStyle,
  withTiming,
  runOnJS,
} from 'react-native-reanimated';
import { AppText } from '../../../components';
import {
  getTypographyStyle,
  longPressMenu,
  spacing,
} from '../../../theme/tokens';
import { getHostShadows } from '../../styles/shadows';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';
import {
  SafeAreaView,
  useSafeAreaInsets,
} from 'react-native-safe-area-context';
import { triggerHaptic } from '../../utils/haptics';

export interface MenuOption {
  label: string;
  onPress: (position?: { x: number; y: number }) => void;
  invokeAfterClose?: boolean;
  icon?: React.ReactNode;
  textColor?: string;
}

interface LongPressMenuProps {
  children: React.ReactNode;
  options: MenuOption[];
  disabled?: boolean;
  onPress?: () => void;
  /**
   * 中文注释：启用后内部不使用 Pressable 包裹，改用 RNGH 手势系统检测长按。
   * 适用于内容包含 ScrollView 等需要滚动手势的场景，避免 Pressable 抢夺 responder。
   * 启用后 onPress（短按）不会生效。
   */
  avoidPressable?: boolean;
}

export const LongPressMenu: React.FC<LongPressMenuProps> = ({
  children,
  options,
  disabled,
  onPress,
  avoidPressable = false,
}) => {
  const colors = useColors();
  const insets = useSafeAreaInsets();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);
  const [visible, setVisible] = useState(false);
  const [pressLocation, setPressLocation] = useState<{
    x: number;
    y: number;
  } | null>(null);
  const [menuSize, setMenuSize] = useState({
    width: longPressMenu.width,
    height: 0,
  });
  const triggerRef = useRef<View>(null);
  const pressLocationRef = useRef<{ x: number; y: number } | null>(null);
  const pendingActionRef = useRef<(() => void) | null>(null);

  const backdropOpacity = useSharedValue(0);
  const menuOpacityAnim = useSharedValue(0);
  const backdropAnimatedStyle = useAnimatedStyle(() => ({
    opacity: backdropOpacity.value,
  }));
  const menuAnimatedStyle = useAnimatedStyle(() => ({
    opacity: menuOpacityAnim.value,
  }));

  const resolveWindowPressLocation = useCallback(
    (
      pageX: number | undefined,
      pageY: number | undefined,
      fallback?: { x: number; y: number },
      onResolved?: (position: { x: number; y: number }) => void,
    ) => {
      triggerRef.current?.measureInWindow((windowX, windowY, width, height) => {
        triggerRef.current?.measure((_x, _y, _width, _height, pageLeft, pageTop) => {
          if (pageX !== undefined && pageY !== undefined) {
            // 中文注释：page 坐标和 Modal 所在的 window 坐标并不总是同一个原点，
            // 用宿主容器同时测出两套坐标后求偏移量，才能把触点稳定映射到菜单坐标系。
            const resolvedPosition = {
              x: pageX + (windowX - pageLeft),
              y: pageY + (windowY - pageTop),
            };
            pressLocationRef.current = resolvedPosition;
            onResolved?.(resolvedPosition);
            return;
          }

          const resolvedPosition = fallback ?? {
            x: windowX + width / 2,
            y: windowY + height / 2,
          };
          pressLocationRef.current = resolvedPosition;
          onResolved?.(resolvedPosition);
        });
      });
    },
    [],
  );

  const handleLongPressTrigger = useCallback(
    (pageX?: number, pageY?: number) => {
      if (disabled) return;

      triggerHaptic('impactHeavy');

      resolveWindowPressLocation(
        pageX,
        pageY,
        pressLocationRef.current ?? undefined,
        (resolvedPressLocation) => {
          setPressLocation(resolvedPressLocation);
        },
      );
      triggerRef.current?.measureInWindow((x, y, width, height) => {
        if (!pressLocationRef.current) {
          pressLocationRef.current = {
            x: x + width / 2,
            y: y + height / 2,
          };
          setPressLocation(pressLocationRef.current);
        }
        setMenuSize((prev) => ({ ...prev, height: 0 }));
        backdropOpacity.value = 0;
        menuOpacityAnim.value = 0;
        setVisible(true);
      });
    },
    [disabled, resolveWindowPressLocation],
  );

  const rngLongPressGesture = React.useMemo(
    () =>
      Gesture.LongPress()
        .minDuration(400)
        .onStart((event) => {
          runOnJS(handleLongPressTrigger)(event.absoluteX, event.absoluteY);
        }),
    [handleLongPressTrigger],
  );

  const handlePressIn = (event: GestureResponderEvent) => {
    if (disabled) return;

    // 中文注释：优先记录手指落点，后续菜单以这个锚点定位，而不是依赖整个气泡的位置。
    const nativeEvent = event?.nativeEvent;
    if (nativeEvent) {
      if (
        nativeEvent.pageX !== undefined &&
        nativeEvent.pageY !== undefined
      ) {
        resolveWindowPressLocation(
          nativeEvent.pageX,
          nativeEvent.pageY,
        );
      }
    }
  };

  const handleLongPress = (event: GestureResponderEvent) => {
    if (disabled) return;

    const nativeEvent = event?.nativeEvent;
    const pageX = nativeEvent?.pageX;
    const pageY = nativeEvent?.pageY;

    if (pageX !== undefined && pageY !== undefined) {
      resolveWindowPressLocation(pageX, pageY);
    }

    handleLongPressTrigger(pageX, pageY);
  };

  const closeMenu = (callback?: () => void) => {
    if (callback) {
      pendingActionRef.current = callback;
    }
    backdropOpacity.value = withTiming(0, { duration: 150 });
    menuOpacityAnim.value = withTiming(0, { duration: 150 }, (isFinished) => {
      if (isFinished) {
        runOnJS(cleanupAfterClose)();
      }
    });
  };

  const handleBackdropPress = () => {
    closeMenu();
  };

  const cleanupAfterClose = () => {
    setVisible(false);
    setPressLocation(null);
    pressLocationRef.current = null;
    if (pendingActionRef.current) {
      const pendingAction = pendingActionRef.current;
      pendingActionRef.current = null;
      pendingAction();
    }
  };

  useEffect(() => {
    if (visible) {
      backdropOpacity.value = withTiming(1, { duration: 200 });
    }
  }, [visible]);

  const renderMenu = () => {
    if (!pressLocation) return null;

    const screenHeight = Dimensions.get('window').height;
    const screenWidth = Dimensions.get('window').width;
    const horizontalMargin = 20;
    const verticalMargin = 20;
    const menuVisualSafeBuffer = 12;
    const safeAreaContentHeight = Math.max(
      screenHeight - insets.top - insets.bottom,
      0,
    );
    const topBoundary = Math.max(
      verticalMargin + menuVisualSafeBuffer,
      verticalMargin,
    );
    const bottomBoundary =
      safeAreaContentHeight -
      Math.max(verticalMargin + menuVisualSafeBuffer, verticalMargin);

    const menuWidth = longPressMenu.width;
    const estimatedMenuHeight =
      longPressMenu.padding * 2 +
      options.length * (spacing.md * 2 + 24) +
      Math.max(0, options.length - 1) * spacing.xxs;
    const menuHeight = menuSize.height || estimatedMenuHeight;
    const safeAreaRelativePressY = pressLocation.y - insets.top;

    // 中文注释：Modal 内容层先被 SafeAreaView 限制在安全区内，
    // 这里再把触点 Y 转成安全区内部坐标，避免菜单进入刘海/灵动岛区域。
    const centeredTop = safeAreaRelativePressY - menuHeight / 2;
    const maxTop = Math.max(topBoundary, bottomBoundary - menuHeight);
    const top = Math.min(Math.max(centeredTop, topBoundary), maxTop);

    // 中文注释：横向以手指位置居中，同时保证菜单留在屏幕可见区域内。
    const left = Math.min(
      Math.max(pressLocation.x - menuWidth / 2, horizontalMargin),
      screenWidth - horizontalMargin - menuWidth,
    );
    return (
      <Modal
        visible={visible}
        transparent
        animationType="none"
        onRequestClose={handleBackdropPress}
      >
        <TouchableWithoutFeedback onPress={handleBackdropPress}>
          <SafeAreaView
            style={styles.modalSafeArea}
            edges={['top', 'bottom', 'left', 'right']}
          >
            <Animated.View
              style={[styles.modalBackdrop, backdropAnimatedStyle]}
            >
              <Animated.View
                style={[
                  styles.menuContainer,
                  menuAnimatedStyle,
                  {
                    top,
                    left,
                    width: menuWidth,
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
                  if (height > 0) {
                    menuOpacityAnim.value = withTiming(1, { duration: 150 });
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
                      const pressPosition =
                        pressLocationRef.current ?? undefined;
                      // 中文注释：二级菜单需要等一级菜单完全收起后再打开，
                      // 否则两个 Modal 的蒙版会在同一时刻重叠，导致后开的菜单看不到且后续触摸被吞掉。
                      if (option.invokeAfterClose) {
                        closeMenu(() => {
                          option.onPress(pressPosition);
                        });
                        return;
                      }
                      option.onPress(pressPosition);
                      closeMenu();
                    }}
                  >
                    {option.icon}
                    <AppText
                      variant="menuLabel"
                      style={[
                        styles.menuText,
                        option.textColor && { color: option.textColor },
                      ]}
                    >
                      {option.label}
                    </AppText>
                  </Pressable>
                ))}
              </Animated.View>
            </Animated.View>
          </SafeAreaView>
        </TouchableWithoutFeedback>
      </Modal>
    );
  };

  if (avoidPressable) {
    // 中文注释：用 RNGH 手势系统检测长按，不包裹 Pressable，避免抢夺 ScrollView 等内部组件的滚动手势。
    return (
      <>
        <GestureDetector gesture={rngLongPressGesture}>
          <View ref={triggerRef} collapsable={false}>
            {children}
          </View>
        </GestureDetector>
        {renderMenu()}
      </>
    );
  }

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
          delayLongPress={200}
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

const makeStyles = (colors: ColorTokens) => {
  const hostShadows = getHostShadows(colors);

  return StyleSheet.create({
    modalSafeArea: {
      flex: 1,
    },
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
      ...hostShadows.menuSurface,
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
};
