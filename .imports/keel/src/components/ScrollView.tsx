/**
 * ScrollView Component
 *
 * Default scroll view component implementation, wrapping React Native ScrollView
 */

import React from 'react';
import {
  ScrollView as RNScrollView,
  type ViewStyle,
  type NativeScrollEvent,
  type NativeSyntheticEvent,
} from 'react-native';

export interface ScrollEvent {
  nativeEvent: {
    contentOffset: { x: number; y: number };
    contentSize: { width: number; height: number };
    layoutMeasurement: { width: number; height: number };
  };
}

export interface ScrollViewProps {
  style?: ViewStyle;
  contentContainerStyle?: ViewStyle;
  children?: React.ReactNode;
  horizontal?: boolean;
  showsVerticalScrollIndicator?: boolean;
  showsHorizontalScrollIndicator?: boolean;
  pagingEnabled?: boolean;
  bounces?: boolean;
  scrollEnabled?: boolean;
  scrollEventThrottle?: number;
  keyboardShouldPersistTaps?: 'always' | 'never' | 'handled';
  keyboardDismissMode?: 'none' | 'on-drag' | 'interactive';
  testID?: string;
  onScroll?: (event: ScrollEvent) => void;
  onScrollBeginDrag?: (event: ScrollEvent) => void;
  onScrollEndDrag?: (event: ScrollEvent) => void;
  onMomentumScrollBegin?: (event: ScrollEvent) => void;
  onMomentumScrollEnd?: (event: ScrollEvent) => void;
}

export function ScrollView({
  style,
  contentContainerStyle,
  children,
  horizontal = false,
  showsVerticalScrollIndicator = true,
  showsHorizontalScrollIndicator = true,
  pagingEnabled = false,
  bounces = true,
  scrollEnabled = true,
  scrollEventThrottle = 16,
  keyboardShouldPersistTaps,
  keyboardDismissMode,
  testID,
  onScroll,
  onScrollBeginDrag,
  onScrollEndDrag,
  onMomentumScrollBegin,
  onMomentumScrollEnd,
}: ScrollViewProps): React.ReactElement {
  // Convert event format
  const handleScroll = onScroll
    ? (event: NativeSyntheticEvent<NativeScrollEvent>) => {
        onScroll({
          nativeEvent: {
            contentOffset: event.nativeEvent.contentOffset,
            contentSize: event.nativeEvent.contentSize,
            layoutMeasurement: event.nativeEvent.layoutMeasurement,
          },
        });
      }
    : undefined;

  const handleScrollBeginDrag = onScrollBeginDrag
    ? (event: NativeSyntheticEvent<NativeScrollEvent>) => {
        onScrollBeginDrag({
          nativeEvent: {
            contentOffset: event.nativeEvent.contentOffset,
            contentSize: event.nativeEvent.contentSize,
            layoutMeasurement: event.nativeEvent.layoutMeasurement,
          },
        });
      }
    : undefined;

  const handleScrollEndDrag = onScrollEndDrag
    ? (event: NativeSyntheticEvent<NativeScrollEvent>) => {
        onScrollEndDrag({
          nativeEvent: {
            contentOffset: event.nativeEvent.contentOffset,
            contentSize: event.nativeEvent.contentSize,
            layoutMeasurement: event.nativeEvent.layoutMeasurement,
          },
        });
      }
    : undefined;

  const handleMomentumScrollBegin = onMomentumScrollBegin
    ? (event: NativeSyntheticEvent<NativeScrollEvent>) => {
        onMomentumScrollBegin({
          nativeEvent: {
            contentOffset: event.nativeEvent.contentOffset,
            contentSize: event.nativeEvent.contentSize,
            layoutMeasurement: event.nativeEvent.layoutMeasurement,
          },
        });
      }
    : undefined;

  const handleMomentumScrollEnd = onMomentumScrollEnd
    ? (event: NativeSyntheticEvent<NativeScrollEvent>) => {
        onMomentumScrollEnd({
          nativeEvent: {
            contentOffset: event.nativeEvent.contentOffset,
            contentSize: event.nativeEvent.contentSize,
            layoutMeasurement: event.nativeEvent.layoutMeasurement,
          },
        });
      }
    : undefined;

  return (
    <RNScrollView
      style={style}
      contentContainerStyle={contentContainerStyle}
      horizontal={horizontal}
      showsVerticalScrollIndicator={showsVerticalScrollIndicator}
      showsHorizontalScrollIndicator={showsHorizontalScrollIndicator}
      pagingEnabled={pagingEnabled}
      bounces={bounces}
      scrollEnabled={scrollEnabled}
      scrollEventThrottle={scrollEventThrottle}
      keyboardShouldPersistTaps={keyboardShouldPersistTaps}
      keyboardDismissMode={keyboardDismissMode}
      testID={testID}
      onScroll={handleScroll}
      onScrollBeginDrag={handleScrollBeginDrag}
      onScrollEndDrag={handleScrollEndDrag}
      onMomentumScrollBegin={handleMomentumScrollBegin}
      onMomentumScrollEnd={handleMomentumScrollEnd}
    >
      {children}
    </RNScrollView>
  );
}

export default ScrollView;
