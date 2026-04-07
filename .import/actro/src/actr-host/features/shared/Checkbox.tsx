import React from 'react';
import { View, StyleSheet, Pressable } from 'react-native';

interface CheckboxProps {
  checked: boolean;
  onPress: () => void;
  size?: number;
}

export const Checkbox: React.FC<CheckboxProps> = ({
  checked,
  onPress,
  size = 20,
}) => {
  return (
    <Pressable
      onPress={onPress}
      style={({ pressed }) => [
        styles.container,
        {
          width: size,
          height: size,
          borderRadius: size / 2,
        },
        pressed && styles.pressed,
      ]}
    >
      <View
        style={[
          styles.checkbox,
          {
            width: size,
            height: size,
            borderRadius: size / 2,
            borderWidth: checked ? 0 : 2,
            backgroundColor: checked ? '#34A8FB' : 'transparent',
            borderColor: checked ? '#34A8FB' : '#D9D9D9',
          },
        ]}
      />
    </Pressable>
  );
};

const styles = StyleSheet.create({
  container: {
    justifyContent: 'center',
    alignItems: 'center',
  },
  checkbox: {
    justifyContent: 'center',
    alignItems: 'center',
  },
  pressed: {
    opacity: 0.7,
  },
});

