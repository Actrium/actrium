/**
 * DropdownMenu - Host Implementation
 *
 * A reusable dropdown menu component with customizable trigger and items.
 */

import React, { useState } from 'react';
import {
  View,
  TouchableOpacity,
  StyleSheet,
  TouchableWithoutFeedback,
  Text,
  Pressable,
} from 'react-native';

interface DropdownMenuProps {
  trigger: React.ReactNode;
  children: React.ReactNode | ((close: () => void) => React.ReactNode);
}

export function DropdownMenu({ trigger, children }: DropdownMenuProps) {
  const [isOpen, setIsOpen] = useState(false);

  const toggle = () => setIsOpen(!isOpen);
  const close = () => setIsOpen(false);

  return (
    <View style={{ zIndex: 1000 }}>
      <TouchableOpacity activeOpacity={0.7} onPress={toggle}>
        {trigger}
      </TouchableOpacity>

      {isOpen && (
        <>
          <TouchableWithoutFeedback onPress={close}>
            <View style={styles.backdrop} />
          </TouchableWithoutFeedback>
          <View style={styles.dropdown}>
            {typeof children === 'function' ? children(close) : children}
          </View>
        </>
      )}
    </View>
  );
}

export function DropdownMenuItem({
  onPress,
  children,
}: {
  onPress: () => void;
  children: React.ReactNode;
}) {
  return (
    <Pressable
      onPress={onPress}
      style={({ pressed }) => [
        styles.dropdownItem,
        { backgroundColor: pressed ? '#F2F2F7' : 'transparent' },
      ]}
    >
      {children}
    </Pressable>
  );
}

export function DropdownMenuText({ children }: { children: string }) {
  return <Text style={styles.dropdownText}>{children}</Text>;
}

export function DropdownMenuSubText({ children }: { children: string }) {
  return <Text style={styles.dropdownSubText}>{children}</Text>;
}

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
    justifyContent: 'flex-start',
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

export default DropdownMenu;
