import React from 'react';
import {
  Platform,
  SafeAreaView,
  StyleProp,
  StyleSheet,
  ViewStyle,
} from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

interface SafeBottomViewProps
  extends Omit<React.ComponentProps<typeof SafeAreaView>, 'style'> {
  style?: StyleProp<ViewStyle>;
  minBottom?: number;
}

export default function SafeBottomView(props: SafeBottomViewProps) {
  const { style, minBottom = 0, children, ...rest } = props;
  const insets = useSafeAreaInsets();

  const extraBottom =
    Platform.OS === 'android' ? Math.max(insets.bottom, minBottom) : 0;
  const flattenedStyle = StyleSheet.flatten(style);
  const basePaddingBottom =
    flattenedStyle && typeof flattenedStyle.paddingBottom === 'number'
      ? flattenedStyle.paddingBottom
      : 0;

  return (
    <SafeAreaView
      {...rest}
      style={[style, { paddingBottom: basePaddingBottom + extraBottom }]}
    >
      {children}
    </SafeAreaView>
  );
}
