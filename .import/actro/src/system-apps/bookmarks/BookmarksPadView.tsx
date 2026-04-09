import React from 'react';
import {
  View,
  ScrollView,
  StyleSheet,
  Image,
  ActivityIndicator,
  TouchableOpacity,
  Pressable,
  useWindowDimensions,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { AppText } from '../../components';
import { getTypographyStyle, spacing, radius } from '../../theme/tokens';
import type { TBookmarkItem } from '../../repository/BookmarkRepository';
import type { UseBookmarksPageResult } from './useBookmarksPage';

// Pad 端固定展示 4 列，但宽度必须按真实可用空间计算，不能再写死设计稿百分比。
const BOOKMARK_GRID_COLUMNS = 4;
const BOOKMARK_HORIZONTAL_PADDING = 20;
const BOOKMARK_GRID_COLUMN_GAP = 16;
const BOOKMARK_GRID_ROW_GAP = 16;

export default function BookmarksPadView({
  groups,
  isLoading,
  error,
  hasBookmarks,
  loadBookmarks,
  openBookmark,
  removeBookmark,
}: UseBookmarksPageResult) {
  const { t } = useTranslation();
  const { width } = useWindowDimensions();
  const flatBookmarks = React.useMemo(
    () =>
      groups.flatMap((group) => group.items).filter((item) => Boolean(item.instance_name)),
    [groups],
  );
  const cardWidth = React.useMemo(() => {
    const contentWidth = Math.max(width - BOOKMARK_HORIZONTAL_PADDING * 2, 0);

    return (
      contentWidth -
      BOOKMARK_GRID_COLUMN_GAP * (BOOKMARK_GRID_COLUMNS - 1)
    ) / BOOKMARK_GRID_COLUMNS;
  }, [width]);

  const handleRemoveBookmark = React.useCallback(
    async (item: TBookmarkItem) => {
      await removeBookmark(item);
    },
    [removeBookmark],
  );

  if (isLoading) {
    return (
      <View style={styles.container}>
        <View style={styles.centerContainer}>
          <ActivityIndicator size="large" color="#34A8FB" />
          <AppText style={styles.centerText}>{t('common.loading')}</AppText>
        </View>
      </View>
    );
  }

  if (error) {
    return (
      <View style={styles.container}>
        <View style={styles.centerContainer}>
          <AppText style={styles.errorText}>{error}</AppText>
          <TouchableOpacity style={styles.retryButton} onPress={loadBookmarks}>
            <AppText style={styles.retryButtonText}>{t('common.retry')}</AppText>
          </TouchableOpacity>
        </View>
      </View>
    );
  }

  if (!hasBookmarks) {
    return (
      <View style={styles.container}>
        <View style={styles.centerContainer}>
          <AppText style={styles.emptyText}>{t('actr.bookmark.empty')}</AppText>
          <AppText style={styles.emptySubText}>{t('actr.bookmark.emptyHint')}</AppText>
        </View>
      </View>
    );
  }

  return (
    <View style={styles.container}>
      <ScrollView
        style={styles.content}
        contentContainerStyle={styles.scrollContent}
        showsVerticalScrollIndicator={false}
      >
        <View style={styles.grid}>
          {flatBookmarks.map((item) => (
            <TouchableOpacity
              key={item.id}
              style={[styles.card, { width: cardWidth }]}
              activeOpacity={0.8}
              onPress={() => openBookmark(item)}
            >
              <View style={styles.cardInner}>
                <Image source={{ uri: item.logo! }} style={styles.cardLogo} />
                <AppText style={styles.cardTitle} numberOfLines={1}>
                  {item.instance_name}
                </AppText>
                <Pressable
                  hitSlop={8}
                  style={styles.removeButton}
                  onPress={(event) => {
                    event.stopPropagation();
                    handleRemoveBookmark(item);
                  }}
                >
                  <AppText style={styles.removeButtonText}>×</AppText>
                </Pressable>
              </View>
            </TouchableOpacity>
          ))}
        </View>
      </ScrollView>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#000',
  },
  content: {
    flex: 1,
  },
  scrollContent: {
    paddingTop: 30,
    paddingHorizontal: BOOKMARK_HORIZONTAL_PADDING,
    paddingBottom: 48,
  },
  grid: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    columnGap: BOOKMARK_GRID_COLUMN_GAP,
    rowGap: BOOKMARK_GRID_ROW_GAP,
  },
  card: {
  },
  cardInner: {
    height: 56,
    borderRadius: radius.xl,
    backgroundColor: '#3C3C4399',
    flexDirection: 'row',
    alignItems: 'center',
    paddingVertical: spacing.md,
    paddingHorizontal: 13,
    paddingRight: 38,
  },
  cardLogo: {
    width: 32,
    height: 32,
    borderRadius: 16,
    backgroundColor: '#2a2a2a',
  },
  cardTitle: {
    flex: 1,
    marginLeft: spacing.md,
    color: '#F2F2F7',
    ...getTypographyStyle('title3'),
    fontWeight: '500',
  },
  removeButton: {
    position: 'absolute',
    top: 18,
    right: 10,
    width: 20,
    height: 20,
    alignItems: 'center',
    justifyContent: 'center',
  },
  removeButtonText: {
    color: '#F2F2F7',
    fontSize: 18,
    lineHeight: 20,
  },
  centerContainer: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    paddingHorizontal: 24,
  },
  centerText: {
    marginTop: spacing.md,
    ...getTypographyStyle('subhead'),
    color: '#8E8E93',
  },
  errorText: {
    ...getTypographyStyle('subhead'),
    color: '#FF453A',
    textAlign: 'center',
    marginBottom: spacing.lg,
  },
  retryButton: {
    paddingHorizontal: 24,
    paddingVertical: 10,
    backgroundColor: '#34A8FB',
    borderRadius: 20,
  },
  retryButtonText: {
    ...getTypographyStyle('subhead'),
    color: '#fff',
    fontWeight: '500',
  },
  emptyText: {
    ...getTypographyStyle('title3'),
    color: '#8E8E93',
    marginBottom: spacing.sm,
  },
  emptySubText: {
    ...getTypographyStyle('body'),
    color: '#636366',
  },
});
