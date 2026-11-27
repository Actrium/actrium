/**
 * ScrollView Component
 *
 * Default scroll view component implementation, wrapping React Native ScrollView
 * Uses forwardRef to allow parent components to get native view reference
 */

import React from 'react';
import {
  type NativeScrollEvent,
  type NativeSyntheticEvent,
  ScrollView as RNScrollView,
  type ViewStyle,
} from 'react-native';
import type { LayoutEvent, ScrollRequest } from '../../../shared/style-types';
import { InitialScrollContentContext, useInitialScroll } from './initial-scroll-content';

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
  scrollRequest?: ScrollRequest;
  scrollToEndOnLayout?: boolean;
  initialScrollPosition?: 'start' | 'end';
  onInitialScrollComplete?: (offset: { x: number; y: number }) => void;
  keyboardShouldPersistTaps?: 'always' | 'never' | 'handled';
  keyboardDismissMode?: 'none' | 'on-drag' | 'interactive';
  testID?: string;
  onLayout?: (event: LayoutEvent) => void;
  onContentSizeChange?: (width: number, height: number) => void;
  onScroll?: (event: ScrollEvent) => void;
  onScrollBeginDrag?: (event: ScrollEvent) => void;
  onScrollEndDrag?: (event: ScrollEvent) => void;
  onMomentumScrollBegin?: (event: ScrollEvent) => void;
  onMomentumScrollEnd?: (event: ScrollEvent) => void;
}

export const ScrollView = React.forwardRef<
  React.ComponentRef<typeof RNScrollView>,
  ScrollViewProps
>(
  (
    {
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
      scrollRequest,
      scrollToEndOnLayout,
      initialScrollPosition,
      onInitialScrollComplete,
      keyboardShouldPersistTaps,
      keyboardDismissMode,
      testID,
      onLayout,
      onContentSizeChange,
      onScroll,
      onScrollBeginDrag,
      onScrollEndDrag,
      onMomentumScrollBegin,
      onMomentumScrollEnd,
    },
    ref
  ) => {
    const initial = useInitialScroll(initialScrollPosition, horizontal, onInitialScrollComplete);
    const nativeRef = useScrollRequest(ref, initial.hidden ? undefined : scrollRequest);
    const followLayout = scrollToEndOnLayout === true && !initial.hidden;
    const scheduleLayoutScroll = useScrollToEndOnLayout(nativeRef, followLayout);
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

    const handleLayout =
      initial.hidden || onLayout || followLayout
        ? ({ nativeEvent }: LayoutEvent) => {
            if (initial.hidden) initial.setViewport(nativeEvent.layout);
            scheduleLayoutScroll();
            onLayout?.({ nativeEvent: { layout: nativeEvent.layout } });
          }
        : undefined;
    const handleContentSizeChange =
      initial.hidden || onContentSizeChange || followLayout
        ? (width: number, height: number) => {
            if (initial.hidden) initial.setContent(width, height);
            scheduleLayoutScroll();
            onContentSizeChange?.(width, height);
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
        ref={nativeRef}
        style={initial.hidden ? [style, { opacity: 0 }] : style}
        contentContainerStyle={contentContainerStyle}
        contentOffset={initial.offset}
        pointerEvents={initial.hidden ? 'none' : undefined}
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
        onLayout={handleLayout}
        onContentSizeChange={handleContentSizeChange}
        onScroll={handleScroll}
        onScrollBeginDrag={handleScrollBeginDrag}
        onScrollEndDrag={handleScrollEndDrag}
        onMomentumScrollBegin={handleMomentumScrollBegin}
        onMomentumScrollEnd={handleMomentumScrollEnd}
      >
        <InitialScrollContentContext.Provider value={initial.track}>
          {children}
        </InitialScrollContentContext.Provider>
      </RNScrollView>
    );
  }
);

function useScrollRequest(
  ref: React.ForwardedRef<React.ComponentRef<typeof RNScrollView>>,
  request: ScrollRequest | undefined
) {
  const nativeRef = React.useRef<React.ComponentRef<typeof RNScrollView>>(null);
  const lastId = React.useRef(-1);
  React.useImperativeHandle(ref, () => nativeRef.current!, []);
  React.useLayoutEffect(() => {
    const view = nativeRef.current;
    if (!view || !request || !Number.isSafeInteger(request.id) || request.id <= lastId.current) {
      return;
    }
    const { id, target, animated = false } = request;
    if (typeof animated !== 'boolean' || (target !== 'end' && !Number.isFinite(target))) {
      return;
    }
    lastId.current = id;
    if (target === 'end') view.scrollToEnd({ animated });
    else view.scrollTo({ y: target, animated });
  }, [request]);
  return nativeRef;
}

function useScrollToEndOnLayout(
  view: React.RefObject<React.ComponentRef<typeof RNScrollView> | null>,
  enabled: boolean
) {
  const active = React.useRef(enabled);
  const frame = React.useRef<number | undefined>(undefined);
  React.useLayoutEffect(() => {
    active.current = enabled;
    return () => {
      active.current = false;
      if (frame.current !== undefined) cancelAnimationFrame(frame.current);
      frame.current = undefined;
    };
  }, [enabled]);

  return React.useCallback(() => {
    if (!active.current || frame.current !== undefined) return;
    frame.current = requestAnimationFrame(() => {
      frame.current = undefined;
      if (active.current) view.current?.scrollToEnd({ animated: false });
    });
  }, [view]);
}

ScrollView.displayName = 'ScrollView';

export default ScrollView;
