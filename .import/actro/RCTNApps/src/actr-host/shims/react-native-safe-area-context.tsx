import React from 'react';
import { View } from 'react-native';

export type EdgeInsets = {
  top: number;
  bottom: number;
  left: number;
  right: number;
};

export const SafeAreaProvider: React.FC<React.PropsWithChildren> = ({
  children,
}) => {
  return <View style={{ flex: 1 }}>{children}</View>;
};

export const SafeAreaView: React.FC<
  React.ComponentProps<typeof View>
> = props => {
  return <View {...props} />;
};

export const useSafeAreaInsets = (): EdgeInsets => ({
  top: 0,
  bottom: 0,
  left: 0,
  right: 0,
});

export const initialWindowMetrics = {
  frame: { x: 0, y: 0, width: 0, height: 0 },
  insets: { top: 0, bottom: 0, left: 0, right: 0 },
};

