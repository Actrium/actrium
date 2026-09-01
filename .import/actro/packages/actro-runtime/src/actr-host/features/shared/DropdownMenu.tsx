import React from 'react';
import {
  Animated,
  Dimensions,
  Keyboard,
  Modal,
  Platform,
  Pressable,
  StyleProp,
  StyleSheet,
  TouchableWithoutFeedback,
  View,
  ViewStyle,
} from 'react-native';
import { AppText } from '../../../components';
import { dropdownMenu, getTypographyStyle, spacing } from '../../../theme/tokens';
import { getHostShadows } from '../../styles/shadows';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';
import { triggerHaptic } from '../../utils/haptics';

type AnchorMode = 'center' | 'start' | 'end';
type VerticalPlacement = 'auto' | 'top' | 'bottom';

const DEFAULT_ESTIMATED_WIDTH = dropdownMenu.defaultEstimatedWidth;
const DEFAULT_ESTIMATED_HEIGHT = 120;
const DEFAULT_HORIZONTAL_MARGIN = 20;
const DEFAULT_VERTICAL_MARGIN = 20;
const MENU_FADE_IN_DURATION = 300;

interface DropdownMenuProps {
  trigger: React.ReactNode;
  children:
    | React.ReactNode
    | ((close: (callback?: () => void, options?: { delayMs?: number }) => void) => React.ReactNode);
  triggerType?: 'press' | 'longPress';
  disabled?: boolean;
  onTriggerPress?: () => void;
  onBeforeOpen?: () => void;
  style?: StyleProp<ViewStyle>;
  triggerStyle?: StyleProp<ViewStyle>;
  dropdownStyle?: StyleProp<ViewStyle>;
  width?: number;
  minWidth?: number;
  maxWidth?: number;
  estimatedWidth?: number;
  estimatedHeight?: number;
  horizontalMargin?: number;
  verticalMargin?: number;
  verticalGap?: number;
  anchorMode?: AnchorMode;
  placement?: VerticalPlacement;
}

interface TriggerRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

interface MenuPosition {
  top: number;
  left: number;
  maxWidth: number;
}

const clamp = (value: number, min: number, max: number) => {
  if (max < min) {
    return min;
  }
  return Math.min(Math.max(value, min), max);
};

export const DropdownMenu: React.FC<DropdownMenuProps> = ({
  trigger,
  children,
  triggerType = 'press',
  disabled = false,
  onTriggerPress,
  onBeforeOpen,
  style,
  triggerStyle,
  dropdownStyle,
  width,
  minWidth,
  maxWidth,
  estimatedWidth = DEFAULT_ESTIMATED_WIDTH,
  estimatedHeight = DEFAULT_ESTIMATED_HEIGHT,
  horizontalMargin = DEFAULT_HORIZONTAL_MARGIN,
  verticalMargin = DEFAULT_VERTICAL_MARGIN,
  verticalGap = 8,
  anchorMode = 'center',
  placement = 'auto',
}) => {
  const [isOpen, setIsOpen] = React.useState(false);
  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);
  const [triggerRect, setTriggerRect] = React.useState<TriggerRect | null>(null);
  const [menuSize, setMenuSize] = React.useState({
    width: estimatedWidth,
    height: estimatedHeight,
  });
  const [isMeasured, setIsMeasured] = React.useState(false);
  const triggerRef = React.useRef<View>(null);
  const rafRef = React.useRef<number | null>(null);
  const menuOpacity = React.useRef(new Animated.Value(0)).current;
  const pendingActionRef = React.useRef<(() => void) | null>(null);
  const closeDelayTimerRef = React.useRef<ReturnType<typeof setTimeout> | null>(null);

  const clearCloseDelayTimer = React.useCallback(() => {
    if (closeDelayTimerRef.current) {
      clearTimeout(closeDelayTimerRef.current);
      closeDelayTimerRef.current = null;
    }
  }, []);

  const closeImmediately = React.useCallback((callback?: () => void) => {
    if (typeof callback === 'function') {
      pendingActionRef.current = callback;
    }
    menuOpacity.stopAnimation();
    Animated.timing(menuOpacity, {
      toValue: 0,
      duration: 200,
      useNativeDriver: true,
    }).start(() => {
      setIsOpen(false);
      setIsMeasured(false);
    });
  }, [menuOpacity]);

  const close = React.useCallback(
    (callback?: () => void, options?: { delayMs?: number }) => {
      const delayMs = options?.delayMs ?? 0;
      clearCloseDelayTimer();
      if (delayMs <= 0) {
        closeImmediately(callback);
        return;
      }
      // 仅对显式指定的菜单项延迟收起，避免影响点外部或其他操作项的即时关闭行为。
      closeDelayTimerRef.current = setTimeout(() => {
        closeDelayTimerRef.current = null;
        closeImmediately(callback);
      }, delayMs);
    },
    [clearCloseDelayTimer, closeImmediately],
  );

  const handleModalDismiss = React.useCallback(() => {
    setTriggerRect(null);
    if (pendingActionRef.current) {
      pendingActionRef.current();
      pendingActionRef.current = null;
    }
  }, []);

  // Android 上 onDismiss 不触发，需要在 visible=false 时手动清理并执行 pending callback
  React.useEffect(() => {
    if (Platform.OS !== 'android' || isOpen) {
      return;
    }
    setTriggerRect(null);
    if (pendingActionRef.current) {
      const action = pendingActionRef.current;
      pendingActionRef.current = null;
      // 延迟执行，确保 Modal 关闭后再触发（如相机/文件选择器）
      const timer = setTimeout(action, 100);
      return () => clearTimeout(timer);
    }
  }, [isOpen]);

  React.useEffect(() => {
    return () => {
      clearCloseDelayTimer();
    };
  }, [clearCloseDelayTimer]);

  const measureTrigger = React.useCallback((openWhenMeasured = true) => {
    triggerRef.current?.measureInWindow((x, y, measuredWidth, measuredHeight) => {
      if (measuredWidth <= 0 || measuredHeight <= 0) {
        return;
      }
      setTriggerRect({
        x,
        y,
        width: measuredWidth,
        height: measuredHeight,
      });
      if (openWhenMeasured) {
        setMenuSize({
          width: width ?? estimatedWidth,
          height: estimatedHeight,
        });
        setIsMeasured(false);
        setIsOpen(true);
      }
    });
  }, [estimatedHeight, estimatedWidth, width]);

  const scheduleMeasureTrigger = React.useCallback(() => {
    if (!isOpen || rafRef.current != null) {
      return;
    }
    rafRef.current = requestAnimationFrame(() => {
      rafRef.current = null;
      measureTrigger(false);
    });
  }, [isOpen, measureTrigger]);

  const menuPosition = React.useMemo<MenuPosition | null>(() => {
    if (!triggerRect) {
      return null;
    }

    const screenWidth = Dimensions.get('window').width;
    const screenHeight = Dimensions.get('window').height;
    const maxSafeWidth = Math.max(0, screenWidth - horizontalMargin * 2);
    const requestedWidth = width ?? menuSize.width;
    const nextMaxWidth = Math.min(maxWidth ?? maxSafeWidth, maxSafeWidth);
    const nextMinWidth = Math.min(minWidth ?? 0, nextMaxWidth);
    const resolvedWidth = clamp(requestedWidth, nextMinWidth, nextMaxWidth);
    const resolvedHeight = Math.max(estimatedHeight, menuSize.height || 0);

    let idealLeft = triggerRect.x;
    if (anchorMode === 'center') {
      idealLeft = triggerRect.x + triggerRect.width / 2 - resolvedWidth / 2;
    } else if (anchorMode === 'end') {
      idealLeft = triggerRect.x + triggerRect.width - resolvedWidth;
    }

    const aboveTop = triggerRect.y - resolvedHeight - verticalGap;
    const belowTop = triggerRect.y + triggerRect.height + verticalGap;
    const canPlaceAbove = aboveTop >= verticalMargin;
    const canPlaceBelow = belowTop + resolvedHeight <= screenHeight - verticalMargin;

    let idealTop = belowTop;
    if (placement === 'top') {
      idealTop = aboveTop;
    } else if (placement === 'bottom') {
      idealTop = belowTop;
    } else if (canPlaceAbove || !canPlaceBelow) {
      idealTop = aboveTop;
    }

    const left = clamp(
      idealLeft,
      horizontalMargin,
      screenWidth - horizontalMargin - resolvedWidth,
    );
    const top = clamp(
      idealTop,
      verticalMargin,
      screenHeight - verticalMargin - resolvedHeight,
    );

    return {
      top,
      left,
      maxWidth: nextMaxWidth,
    };
  }, [
    anchorMode,
    estimatedHeight,
    horizontalMargin,
    maxWidth,
    menuSize.height,
    menuSize.width,
    minWidth,
    placement,
    triggerRect,
    verticalGap,
    verticalMargin,
    width,
  ]);

  const toggle = React.useCallback(() => {
    if (disabled) {
      return;
    }
    if (isOpen) {
      close();
      return;
    }
    triggerHaptic('impactHeavy')
    onBeforeOpen?.();
    measureTrigger(true);
  }, [close, disabled, isOpen, measureTrigger, onBeforeOpen]);

  React.useEffect(() => {
    if (!isOpen) {
      return;
    }

    const keyboardShowEvent =
      Platform.OS === 'ios' ? 'keyboardDidChangeFrame' : 'keyboardDidShow';
    const keyboardHideEvent = 'keyboardDidHide';
    const handleLayoutChanged = () => scheduleMeasureTrigger();

    const showSub = Keyboard.addListener(keyboardShowEvent, handleLayoutChanged);
    const hideSub = Keyboard.addListener(keyboardHideEvent, handleLayoutChanged);
    const dimensionSub = Dimensions.addEventListener('change', handleLayoutChanged);

    return () => {
      showSub.remove();
      hideSub.remove();
      dimensionSub.remove();
      if (rafRef.current != null) {
        cancelAnimationFrame(rafRef.current);
        rafRef.current = null;
      }
    };
  }, [isOpen, scheduleMeasureTrigger]);

  React.useEffect(() => {
    if (!isOpen || !isMeasured) {
      return;
    }

    menuOpacity.setValue(0);
    Animated.timing(menuOpacity, {
      toValue: 1,
      duration: MENU_FADE_IN_DURATION,
      useNativeDriver: true,
    }).start();
  }, [isMeasured, isOpen, menuOpacity]);

  return (
    <View ref={triggerRef} collapsable={false} style={style}>
      <Pressable
        delayLongPress={350}
        onPress={triggerType === 'press' ? toggle : onTriggerPress}
        onLongPress={triggerType === 'longPress' ? toggle : undefined}
        disabled={disabled}
        style={({ pressed }) => [
          triggerStyle,
          { opacity: pressed ? 0.7 : disabled ? 0.3 : 1 },
        ]}
      >
        {trigger}
      </Pressable>

      {!!triggerRect && !!menuPosition && (
        <Modal
          visible={isOpen}
          transparent
          animationType='none'
          onRequestClose={() => close()}
          onDismiss={handleModalDismiss}
        >
          <TouchableWithoutFeedback onPress={() => close()}>
            <View style={styles.modalBackdrop}>
              <Animated.View
                pointerEvents='none'
                style={[
                  StyleSheet.absoluteFill,
                  { backgroundColor: 'rgba(0,0,0,0.1)', opacity: isMeasured ? menuOpacity : 0 },
                ]}
              />
              <Animated.View
                style={[
                  styles.dropdown,
                  { backgroundColor: colors.bgSurface },
                  {
                    top: menuPosition.top,
                    left: menuPosition.left,
                    width,
                    minWidth,
                    maxWidth: menuPosition.maxWidth,
                    opacity: isMeasured ? menuOpacity : 0,
                  },
                  dropdownStyle,
                ]}
                onLayout={(event) => {
                  if (width != null) {
                    if (!isMeasured) {
                      setIsMeasured(true);
                    }
                    return;
                  }

                  const { width: nextWidth, height: nextHeight } = event.nativeEvent.layout;
                  const widthChanged = Math.abs(nextWidth - menuSize.width) > 1;
                  const heightChanged = Math.abs(nextHeight - menuSize.height) > 1;

                  if (widthChanged || heightChanged) {
                    setMenuSize({ width: nextWidth, height: nextHeight });
                  }
                  if (!isMeasured) {
                    setIsMeasured(true);
                  }
                }}
              >
                {typeof children === 'function' ? children(close) : children}
              </Animated.View>
            </View>
          </TouchableWithoutFeedback>
        </Modal>
      )}
    </View>
  );
};

export const DropdownMenuItem: React.FC<{
  onPress: () => void;
  children: React.ReactNode;
  isActive?: boolean;
}> = ({ onPress, children, isActive }) => {
  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);
  return (
    <Pressable
      onPress={onPress}
      style={({ pressed }) => [
        styles.dropdownItem,
        {
          backgroundColor: isActive
            ? colors.bgActionButtonActive
            : pressed
            ? colors.bgMenuItemPressed
            : 'transparent',
        },
      ]}
    >
      <View style={styles.dropdownItemContent}>{children}</View>
    </Pressable>
  );
};

export const DropdownMenuText: React.FC<{
  children: string;
  color?: string;
}> = ({ children, color }) => {
  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);
  return (
    <AppText variant='menuLabel' style={[styles.dropdownText, { color: color ?? colors.textMenuLabel }]}>
      {children}
    </AppText>
  );
};

export const DropdownMenuSubText: React.FC<{
  children: string;
}> = ({ children }) => {
  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);
  return (
    <AppText variant='caption1' style={[styles.dropdownSubText, { color: colors.textTertiary }]}>
      {children}
    </AppText>
  );
};

const makeStyles = (colors: ColorTokens) => {
  const hostShadows = getHostShadows(colors);

  return StyleSheet.create({
    modalBackdrop: {
      flex: 1,
    },
    dropdown: {
      position: 'absolute',
      borderRadius: 34,
      padding: dropdownMenu.padding,
      gap: spacing.sm,
      ...hostShadows.menuSurface,
    },
    dropdownItem: {
      flexDirection: 'row',
      alignItems: 'center',
      paddingHorizontal: spacing.lg,
      paddingVertical: spacing.md,
      gap: spacing.sm,
      borderRadius: 34,
      justifyContent: 'space-between',
    },
    dropdownItemContent: {
      flexDirection: 'row',
      alignItems: 'center',
      gap: spacing.md,
    },
    dropdownText: {
      ...getTypographyStyle('menuLabel'),
      fontWeight: '500',
      flexShrink: 1,
    },
    dropdownSubText: {
      ...getTypographyStyle('caption1'),
      marginTop: spacing.xxxs,
      fontWeight: '400',
      flexShrink: 1,
    },
  });
};
