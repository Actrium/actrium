import React from 'react';
import {
  View,
  Text,
  FlatList,
  StyleSheet,
  Image,
  ActivityIndicator,
  TouchableOpacity,
} from 'react-native';
import { SwipeableMethods } from 'react-native-gesture-handler/ReanimatedSwipeable';
import { useTranslation } from 'react-i18next';
import type { TBookmarkItem } from '../../repository/BookmarkRepository';
import { SwipeRemoveActionRow } from '../components/SwipeRemoveActionRow';
import type { UseBookmarksPageResult } from './useBookmarksPage';

type TFlatBookmarkItem = TBookmarkItem & {
  isLastInGroup: boolean;
  isFirstInGroup: boolean;
};

export default function BookmarksPhoneView({
  groups,
  isLoading,
  error,
  hasBookmarks,
  loadBookmarks,
  removeBookmark,
  openBookmark,
}: UseBookmarksPageResult) {
  const { t } = useTranslation();
  const [, setOpenedSwipeItemId] = React.useState<number | null>(null);
  const openedSwipeItemIdRef = React.useRef<number | null>(null);
  const swipeableRefs = React.useRef<Record<number, SwipeableMethods | null>>(
    {},
  );

  const flatBookmarks = React.useMemo<TFlatBookmarkItem[]>(
    () =>
      groups.flatMap((group) =>
        group.items.map((item, index) => ({
          ...item,
          isLastInGroup: index === group.items.length - 1,
          isFirstInGroup: index === 0,
        })),
      ),
    [groups],
  );

  const handleRemoveBookmark = React.useCallback(
    async (item: TBookmarkItem) => {
      swipeableRefs.current[item.id]?.close();
      openedSwipeItemIdRef.current = null;
      setOpenedSwipeItemId(null);
      await removeBookmark(item);
    },
    [removeBookmark],
  );

  const renderBookmarkItem = React.useCallback(
    ({ item }: { item: TFlatBookmarkItem }) => (
      <View
        style={[
          styles.listItemWrapper,
          item.isFirstInGroup && styles.listItemGroupTop,
          item.isLastInGroup && styles.listItemGroupBottom,
        ]}
      >
        <SwipeRemoveActionRow
          itemId={item.id}
          swipeableRefs={swipeableRefs}
          openedSwipeItemIdRef={openedSwipeItemIdRef}
          setOpenedSwipeItemId={setOpenedSwipeItemId}
          onRemovePress={(methods) => {
            methods.close();
            handleRemoveBookmark(item);
          }}
        >
          <TouchableOpacity
            style={styles.listItem}
            onPress={() => openBookmark(item)}
          >
            <Image source={{ uri: item.logo! }} style={styles.listItemLogo} />

            <Text style={styles.listItemName} numberOfLines={1}>
              {item.app_name}
            </Text>
          </TouchableOpacity>
        </SwipeRemoveActionRow>
        {!item.isLastInGroup && <View style={styles.listItemDivider} />}
      </View>
    ),
    [handleRemoveBookmark, openBookmark],
  );

  if (isLoading) {
    return (
      <View style={styles.container}>
        <View style={styles.header}>
          <Text style={styles.headerTitle}>AskAway</Text>
        </View>
        <View style={styles.centerContainer}>
          <ActivityIndicator size="large" color="#34A8FB" />
          <Text style={styles.centerText}>{t('common.loading')}</Text>
        </View>
      </View>
    );
  }

  if (error) {
    return (
      <View style={styles.container}>
        <View style={styles.header}>
          <Text style={styles.headerTitle}>AskAway</Text>
        </View>
        <View style={styles.centerContainer}>
          <Text style={styles.errorText}>{error}</Text>
          <TouchableOpacity style={styles.retryButton} onPress={loadBookmarks}>
            <Text style={styles.retryButtonText}>{t('common.retry')}</Text>
          </TouchableOpacity>
        </View>
      </View>
    );
  }

  if (!hasBookmarks) {
    return (
      <View style={styles.container}>
        <View style={styles.header}>
          <Text style={styles.headerTitle}>AskAway</Text>
        </View>
        <View style={styles.centerContainer}>
          <Text style={styles.emptyText}>{t('actr.bookmark.empty')}</Text>
          <Text style={styles.emptySubText}>{t('actr.bookmark.emptyHint')}</Text>
        </View>
      </View>
    );
  }

  return (
    <View style={styles.container}>
      <View style={styles.header}>
        <Text style={styles.headerTitle}>AskAway</Text>
      </View>

      <FlatList
        style={styles.content}
        contentContainerStyle={styles.scrollContent}
        showsVerticalScrollIndicator={false}
        data={flatBookmarks}
        keyExtractor={(item) => String(item.id)}
        renderItem={renderBookmarkItem}
        initialNumToRender={16}
        maxToRenderPerBatch={20}
        windowSize={10}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#000',
  },
  header: {
    paddingHorizontal: 16,
    paddingTop: 60,
    paddingBottom: 24,
  },
  headerTitle: {
    fontSize: 34,
    color: '#fff',
    marginTop: 8,
  },
  content: {
    flex: 1,
  },
  scrollContent: {
    paddingHorizontal: 16,
    paddingBottom: 40,
  },
  listItemWrapper: {
    backgroundColor: '#1a1a1a',
    overflow: 'hidden',
  },
  listItemGroupTop: {
    borderTopLeftRadius: 13,
    borderTopRightRadius: 13,
    marginTop: 12,
  },
  listItemGroupBottom: {
    borderBottomLeftRadius: 13,
    borderBottomRightRadius: 13,
  },
  listItem: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 20,
    height: 52,
    backgroundColor: '#1a1a1a',
  },
  listItemDivider: {
    height: 1,
    width: '80%',
    backgroundColor: '#000',
    alignSelf: 'center',
  },
  listItemLogo: {
    width: 32,
    height: 32,
    borderRadius: 16,
    backgroundColor: '#2a2a2a',
  },
  listItemName: {
    flex: 1,
    fontSize: 17,
    color: '#fff',
    marginLeft: 12,
    fontWeight: '500',
  },
  centerContainer: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    paddingHorizontal: 24,
  },
  centerText: {
    marginTop: 12,
    fontSize: 15,
    color: '#8E8E93',
  },
  errorText: {
    fontSize: 15,
    color: '#FF453A',
    textAlign: 'center',
    marginBottom: 16,
  },
  retryButton: {
    paddingHorizontal: 24,
    paddingVertical: 10,
    backgroundColor: '#34A8FB',
    borderRadius: 20,
  },
  retryButtonText: {
    fontSize: 15,
    color: '#fff',
    fontWeight: '500',
  },
  emptyText: {
    fontSize: 17,
    color: '#8E8E93',
    marginBottom: 8,
  },
  emptySubText: {
    fontSize: 14,
    color: '#636366',
  },
});
