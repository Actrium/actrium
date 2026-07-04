import React from 'react';
import { Text, TextProps, StyleSheet } from 'react-native';
import {
  getTypographyStyle,
  textBaseStyle,
  TypographyVariant,
} from '../theme/tokens';

interface AppTextProps extends TextProps {
  variant?: TypographyVariant;
}

export function AppText({
  variant = 'body',
  style,
  allowFontScaling = false,
  ...props
}: AppTextProps) {
  return (
    <Text
      {...props}
      allowFontScaling={allowFontScaling}
      style={[styles.base, getTypographyStyle(variant), style]}
    />
  );
}

const styles = StyleSheet.create({
  base: {
    ...textBaseStyle,
  },
});
