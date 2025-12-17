/**
 * View Component
 *
 * Default View component implementation, wrapping React Native View
 */

import React from 'react';
import { View as RNView, type ViewStyle } from 'react-native';

export interface ViewProps {
  style?: ViewStyle;
  children?: React.ReactNode;
  testID?: string;
  pointerEvents?: 'auto' | 'none' | 'box-none' | 'box-only';
  accessible?: boolean;
  accessibilityLabel?: string;
  onLayout?: (event: {
    nativeEvent: {
      layout: { x: number; y: number; width: number; height: number };
    };
  }) => void;
}

export function View({
  style,
  children,
  testID,
  pointerEvents,
  accessible,
  accessibilityLabel,
  onLayout,
}: ViewProps): React.ReactElement {
  return (
    <RNView
      style={style}
      testID={testID}
      pointerEvents={pointerEvents}
      accessible={accessible}
      accessibilityLabel={accessibilityLabel}
      onLayout={onLayout}
    >
      {children}
    </RNView>
  );
}

export default View;
