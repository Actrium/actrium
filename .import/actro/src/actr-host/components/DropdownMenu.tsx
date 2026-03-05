import React, { useState, useRef, useCallback, useEffect } from 'react';
import {
  View,
  StyleSheet,
  TouchableWithoutFeedback,
  Text,
  Pressable,
  ViewStyle,
  StyleProp,
  Modal,
  Dimensions,
  Keyboard,
  Platform,
} from 'react-native';

interface DropdownMenuProps {
  trigger: React.ReactNode;
  children: React.ReactNode | ((close: () => void) => React.ReactNode);
  triggerType?: 'press' | 'longPress';
  verticalGap?: number;
  dropdownStyle?: StyleProp<ViewStyle>;
  style?: StyleProp<ViewStyle>;
  triggerStyle?: StyleProp<ViewStyle>;
  disabled?: boolean;
  onBeforeOpen?: () => void;
}

export const DropdownMenu: React.FC<DropdownMenuProps> = ({
  trigger,
  children,
  triggerType = 'press',
  verticalGap = 8,
  dropdownStyle,
  style,
  triggerStyle,
  disabled = false,
  onBeforeOpen,
}) => {
  const [isOpen, setIsOpen] = useState(false);
  const [triggerRect, setTriggerRect] = useState<{
    x: number;
    y: number;
    width: number;
    height: number;
  } | null>(null);
  const [menuSize, setMenuSize] = useState<{ width: number; height: number }>({
    width: 228,
    height: 120,
  });
  const triggerRef = useRef<View>(null);
  const rafRef = useRef<number | null>(null);

  const calculatePosition = useCallback((openWhenMeasured = true) => {
    triggerRef.current?.measureInWindow((x, y, width, height) => {
      if (width <= 0 || height <= 0) return;
      setTriggerRect({ x, y, width, height });
      if (openWhenMeasured) {
        setIsOpen(true);
      }
    });
  }, []);

  const scheduleRecalculatePosition = useCallback(() => {
    if (!isOpen) return;
    if (rafRef.current != null) return;
    rafRef.current = requestAnimationFrame(() => {
      rafRef.current = null;
      calculatePosition(false);
    });
  }, [calculatePosition, isOpen]);

  const menuPosition = React.useMemo(() => {
    if (!triggerRect) return null;

    const screenWidth = Dimensions.get('window').width;
    const screenHeight = Dimensions.get('window').height;
    const horizontalMargin = 16;
    const verticalMargin = 12;
    const gap = verticalGap;

    const menuWidth = Math.max(228, menuSize.width || 0);
    const menuHeight = Math.max(120, menuSize.height || 0);

    const aboveTop = triggerRect.y - menuHeight - gap;
    const belowTop = triggerRect.y + triggerRect.height + gap;
    const maxTop = Math.max(verticalMargin, screenHeight - verticalMargin - menuHeight);

    // 优先显示在触发器上方；如果上方空间不足再显示到下方。
    const preferredTop = aboveTop >= verticalMargin ? aboveTop : belowTop;
    const top = Math.max(verticalMargin, Math.min(preferredTop, maxTop));

    const centeredLeft = triggerRect.x + triggerRect.width / 2 - menuWidth / 2;
    const maxLeft = screenWidth - menuWidth - horizontalMargin;
    const left = Math.max(horizontalMargin, Math.min(centeredLeft, maxLeft));

    return { top, left };
  }, [menuSize.height, menuSize.width, triggerRect, verticalGap]);

  const toggle = () => {
    if (disabled) return;
    if (isOpen) {
      setIsOpen(false);
    } else {
      onBeforeOpen?.();
      calculatePosition();
    }
  };

  const close = () => {
    setIsOpen(false);
    setTriggerRect(null);
  };

  useEffect(() => {
    if (!isOpen) return;

    const keyboardShowEvent =
      Platform.OS === 'ios' ? 'keyboardDidChangeFrame' : 'keyboardDidShow';
    const keyboardHideEvent = 'keyboardDidHide';
    const onLayoutChanged = () => scheduleRecalculatePosition();

    const showSub = Keyboard.addListener(keyboardShowEvent, onLayoutChanged);
    const hideSub = Keyboard.addListener(keyboardHideEvent, onLayoutChanged);
    const dimensionSub = Dimensions.addEventListener('change', onLayoutChanged);

    return () => {
      showSub.remove();
      hideSub.remove();
      dimensionSub.remove();
      if (rafRef.current != null) {
        cancelAnimationFrame(rafRef.current);
        rafRef.current = null;
      }
    };
  }, [isOpen, scheduleRecalculatePosition]);

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

      {isOpen && triggerRect && menuPosition && (
        <Modal
          visible={isOpen}
          transparent
          animationType="fade"
          onRequestClose={close}
        >
          <TouchableWithoutFeedback onPress={close}>
            <View style={styles.modalBackdrop}>
              <View
                style={[
                  styles.dropdown,
                  {
                    top: menuPosition.top,
                    left: menuPosition.left,
                  },
                  dropdownStyle,
                ]}
                onLayout={(event) => {
                  const { width, height } = event.nativeEvent.layout;
                  if (Math.abs(width - menuSize.width) > 1 || Math.abs(height - menuSize.height) > 1) {
                    setMenuSize({ width, height });
                  }
                }}
              >
                {typeof children === 'function' ? children(close) : children}
              </View>
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
            ? '#D1D1D6' // Even darker than E5E5EA
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
  isActive?: boolean;
  color?: string;
}> = ({ children, isActive, color }) => {
  return (
    <Text style={[styles.dropdownText, color && { color }]}>
      {children}
    </Text>
  );
};

export const DropdownMenuSubText: React.FC<{
  children: string;
  isActive?: boolean;
}> = ({ children, isActive }) => {
  return <Text style={styles.dropdownSubText}>{children}</Text>;
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
    padding: 10,
    minWidth: 228,
    gap: 8,

    // iOS Shadow
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 8 },
    shadowOpacity: 0.12,
    shadowRadius: 16,
    // Android Shadow
    elevation: 8,
  },
  dropdownItem: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 16,
    paddingVertical: 12,
    gap: 8,
    borderRadius: 34,
    justifyContent: 'space-between',
  },
  dropdownItemContent: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
  },
  dropdownText: {
    fontSize: 17,
    color: '#333333',
    fontWeight: '500',
  },
  dropdownSubText: {
    fontSize: 13,
    color: '#999999',
    marginTop: 2,
    fontWeight: '400',
  },
});
