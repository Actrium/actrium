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
import { HOST_SHADOWS } from '../../styles/shadows';

type AnchorMode = 'center' | 'start' | 'end';
type VerticalPlacement = 'auto' | 'top' | 'bottom';

const DEFAULT_ESTIMATED_WIDTH = dropdownMenu.defaultEstimatedWidth;
const DEFAULT_ESTIMATED_HEIGHT = 120;
const DEFAULT_HORIZONTAL_MARGIN = 16;
const DEFAULT_VERTICAL_MARGIN = 12;
const MENU_FADE_IN_DURATION = 300;

interface DropdownMenuProps {
  trigger: React.ReactNode;
  children: React.ReactNode | ((close: (callback?: () => void) => void) => React.ReactNode);
  triggerType?: 'press' | 'longPress';
  disabled?: boolean;
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

  const close = React.useCallback((callback?: () => void) => {
    if (typeof callback === 'function') {
      pendingActionRef.current = callback;
    }
    menuOpacity.stopAnimation();
    menuOpacity.setValue(0);
    setIsOpen(false);
    setIsMeasured(false);
  }, [menuOpacity]);

  const handleModalDismiss = React.useCallback(() => {
    setTriggerRect(null);
    if (pendingActionRef.current) {
      pendingActionRef.current();
      pendingActionRef.current = null;
    }
  }, []);

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
        onPress={triggerType === 'press' ? toggle : () => {}}
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
                style={[
                  styles.dropdown,
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
  return (
    <Pressable
      onPress={onPress}
      style={({ pressed }) => [
        styles.dropdownItem,
        {
          backgroundColor: isActive
            ? '#D1D1D6'
            : pressed
            ? '#F2F2F7'
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
  return (
    <AppText variant='menuLabel' style={[styles.dropdownText, color && { color }]}>
      {children}
    </AppText>
  );
};

export const DropdownMenuSubText: React.FC<{
  children: string;
}> = ({ children }) => {
  return (
    <AppText variant='caption1' style={styles.dropdownSubText}>
      {children}
    </AppText>
  );
};

const styles = StyleSheet.create({
  modalBackdrop: {
    flex: 1,
    backgroundColor: 'transparent',
  },
  dropdown: {
    position: 'absolute',
    backgroundColor: '#FFFFFF',
    borderRadius: 34,
    padding: dropdownMenu.padding,
    gap: spacing.sm,
    ...HOST_SHADOWS.menuSurface,
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
    color: '#333333',
    fontWeight: '500',
    flexShrink: 1,
  },
  dropdownSubText: {
    ...getTypographyStyle('caption1'),
    color: '#999999',
    marginTop: spacing.xxxs,
    fontWeight: '400',
    flexShrink: 1,
  },
});
