import React, { useState, useRef, useCallback } from 'react';
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
} from 'react-native';

interface DropdownMenuProps {
  trigger: React.ReactNode;
  children: React.ReactNode | ((close: () => void) => React.ReactNode);
  triggerType?: 'press' | 'longPress';
  dropdownStyle?: StyleProp<ViewStyle>;
  style?: StyleProp<ViewStyle>;
  triggerStyle?: StyleProp<ViewStyle>;
  disabled?: boolean;
}

export const DropdownMenu: React.FC<DropdownMenuProps> = ({
  trigger,
  children,
  triggerType = 'press',
  dropdownStyle,
  style,
  triggerStyle,
  disabled = false,
}) => {
  const [isOpen, setIsOpen] = useState(false);
  const [menuPosition, setMenuPosition] = useState<{ top: number; left: number } | null>(null);
  const triggerRef = useRef<View>(null);

  const calculatePosition = useCallback(() => {
    triggerRef.current?.measureInWindow((x, y, width, height) => {
      const screenWidth = Dimensions.get('window').width;
      const screenHeight = Dimensions.get('window').height;
      const menuWidth = 228;
      const estimatedMenuHeight = 120; // 菜单估算高度
      const gap = 8; // 菜单与按钮的间距

      // 默认显示在触发器上方，紧贴按钮
      let top = y - estimatedMenuHeight - gap;
      let left = x + width / 2 - menuWidth / 2;

      // 如果上方空间不足，显示在下方
      if (top < 20) {
        top = y + height + gap;
      }

      // 左右边界检查
      if (left + menuWidth > screenWidth - 16) {
        left = screenWidth - menuWidth - 16;
      }
      if (left < 16) left = 16;

      setMenuPosition({ top, left });
      setIsOpen(true);
    });
  }, []);

  const toggle = () => {
    if (disabled) return;
    if (isOpen) {
      setIsOpen(false);
    } else {
      calculatePosition();
    }
  };

  const close = () => {
    setIsOpen(false);
    setMenuPosition(null);
  };

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

      {isOpen && menuPosition && (
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
                ]}
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
