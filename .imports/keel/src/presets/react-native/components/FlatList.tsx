/**
 * FlatList Component
 *
 * Default FlatList component implementation, wrapping React Native FlatList
 */

import type React from 'react';
import { type ListRenderItem, FlatList as RNFlatList, type ViewStyle } from 'react-native';

export interface FlatListProps<T = unknown> {
  data: T[];
  renderItem: ListRenderItem<T>;
  keyExtractor?: (item: T, index: number) => string;
  style?: ViewStyle;
  contentContainerStyle?: ViewStyle;
  horizontal?: boolean;
  numColumns?: number;
  initialNumToRender?: number;
  maxToRenderPerBatch?: number;
  windowSize?: number;
  removeClippedSubviews?: boolean;
  showsVerticalScrollIndicator?: boolean;
  showsHorizontalScrollIndicator?: boolean;
  onEndReached?: () => void;
  onEndReachedThreshold?: number;
  onRefresh?: () => void;
  refreshing?: boolean;
  ListHeaderComponent?: React.ReactElement | null;
  ListFooterComponent?: React.ReactElement | null;
  ListEmptyComponent?: React.ReactElement | null;
  ItemSeparatorComponent?: React.ReactElement | null;
  testID?: string;
  accessible?: boolean;
  accessibilityLabel?: string;
}

export function FlatList<T = unknown>({
  data,
  renderItem,
  keyExtractor,
  style,
  contentContainerStyle,
  horizontal,
  numColumns,
  initialNumToRender,
  maxToRenderPerBatch,
  windowSize,
  removeClippedSubviews,
  showsVerticalScrollIndicator,
  showsHorizontalScrollIndicator,
  onEndReached,
  onEndReachedThreshold,
  onRefresh,
  refreshing,
  ListHeaderComponent,
  ListFooterComponent,
  ListEmptyComponent,
  ItemSeparatorComponent,
  testID,
  accessible,
  accessibilityLabel,
}: FlatListProps<T>): React.ReactElement {
  return (
    <RNFlatList
      data={data}
      renderItem={renderItem}
      keyExtractor={keyExtractor}
      style={style}
      contentContainerStyle={contentContainerStyle}
      horizontal={horizontal}
      numColumns={numColumns}
      initialNumToRender={initialNumToRender}
      maxToRenderPerBatch={maxToRenderPerBatch}
      windowSize={windowSize}
      removeClippedSubviews={removeClippedSubviews}
      showsVerticalScrollIndicator={showsVerticalScrollIndicator}
      showsHorizontalScrollIndicator={showsHorizontalScrollIndicator}
      onEndReached={onEndReached}
      onEndReachedThreshold={onEndReachedThreshold}
      onRefresh={onRefresh}
      refreshing={refreshing}
      ListHeaderComponent={ListHeaderComponent}
      ListFooterComponent={ListFooterComponent}
      ListEmptyComponent={ListEmptyComponent}
      ItemSeparatorComponent={
        ItemSeparatorComponent as React.ComponentType<unknown> | null | undefined
      }
      testID={testID}
      accessible={accessible}
      accessibilityLabel={accessibilityLabel}
    />
  );
}

export default FlatList;
