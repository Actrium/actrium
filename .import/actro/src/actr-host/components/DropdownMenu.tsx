import React, { useState } from 'react';
import {
  View,
  TouchableOpacity,
  StyleSheet,
  TouchableWithoutFeedback,
  Text,
  Pressable,
  ViewStyle,
  StyleProp,
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

  const toggle = () => {
    if (disabled) return;
    console.log('[DropdownMenu] Toggle called, current state:', !isOpen, 'type:', triggerType);
    setIsOpen(!isOpen);
  };
  const close = () => setIsOpen(false);

  return (
    <View style={[{ zIndex: isOpen ? 9999 : 1000 }, style]}>
      <Pressable
        delayLongPress={350} // 稍微缩短，变得更灵敏
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

      {isOpen && (
        <>
          <TouchableWithoutFeedback onPress={close}>
            <View style={styles.backdrop} />
          </TouchableWithoutFeedback>
          <View style={[styles.dropdown, dropdownStyle]}>
            {typeof children === 'function' ? children(close) : children}
          </View>
        </>
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
  dropdown: {
    position: 'absolute',
    bottom: 50,
    left: 0,
    backgroundColor: '#FFFFFF',
    borderRadius: 34,
    padding: 10,
    minWidth: 228,
    zIndex: 1001,
    gap: 8,

    // iOS Shadow
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 8 },
    shadowOpacity: 0.12,
    shadowRadius: 16,
    // Android Shadow
    elevation: 8,
  },
  backdrop: {
    position: 'absolute',
    top: -1000,
    left: -1000,
    right: -1000,
    bottom: -1000,
    backgroundColor: 'transparent',
    zIndex: 1000,
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
