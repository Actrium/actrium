import React from 'react';
import { View, StyleSheet, Pressable } from 'react-native';
import { useColors } from '../../../theme/useColors';
import { Icon } from '../../../components';

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
  const colors = useColors();
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
            backgroundColor: checked ? colors.accentBlue : 'transparent',
            borderColor: checked ? colors.accentBlue : colors.borderCheckboxUnchecked,
          },
        ]}
      >
        {checked && (
          <Icon type='success' size={size * 0.5} color='white' />
        )}
      </View>
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

