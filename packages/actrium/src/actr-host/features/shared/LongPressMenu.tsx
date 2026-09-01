import React, { useState, useRef, useCallback, useEffect } from 'react';
import {
  View,
  StyleSheet,
  Pressable,
  Modal,
  TouchableWithoutFeedback,
  Dimensions,
  GestureResponderEvent,
  InteractionManager,
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
   * Comment in Chinese: Not used internally after enabled Pressable Package, change of page. RNGH Gear system check long.
   * Applicable to content inclusion ScrollView Wait for the scenes that need to roll. Avoid. Pressable Take it. responder.
   * After Enabled onPress(Small) does not take effect.
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
            // page Coordinates and Modal Where it is. window The coordinates are not always the same as the original.
            // The two sets of coordinates are measured in the host packaging and offsets are sought to stabilize the contact to the menu coordinates.
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

    // The priority is to record the point of drop of the finger, and the subsequent menu is positioned at this anchor rather than depending on the location of the entire bubble.
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
    backdropOpacity.value = withTiming(0, { duration: 100 });
    menuOpacityAnim.value = withTiming(0, { duration: 100 }, (isFinished) => {
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
      // Wait Modal It's... visible=false Submit and let the original close interactively and then execute focused actions.
      requestAnimationFrame(() => {
        InteractionManager.runAfterInteractions(() => {
          pendingAction();
        });
      });
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

    // Modal Content Layer First SafeAreaView The government has been monitoring the situation in the country.
    // Here, put another touch. Y Turn to the inside coordinates of the security zone and avoid the menus entering Liu Hai./The Peripheral Island region.
    const centeredTop = safeAreaRelativePressY - menuHeight / 2;
    const maxTop = Math.max(topBoundary, bottomBoundary - menuHeight);
    const top = Math.min(Math.max(centeredTop, topBoundary), maxTop);

    // Horizontally centered by finger position, while ensuring that the menu remains in the visible area of the screen.
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
                      // The second menu needs to be opened after the first menu is completely closed.
                      // Or two. Modal The masked version will overlap at the same time, resulting in the later menu not being seen and subsequent touch being swallowed.
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
    // Use RNGH The signal system is long, no package. PressableAvoid the grab. ScrollView Wait for the roll-on of the internal component.
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
