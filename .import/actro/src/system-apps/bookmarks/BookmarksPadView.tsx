import React from 'react';
import {
  View,
  Text,
  ScrollView,
  StyleSheet,
  Image,
  ActivityIndicator,
  TouchableOpacity,
  Pressable,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import type { TBookmarkItem } from '../../repository/BookmarkRepository';
import type { UseBookmarksPageResult } from './useBookmarksPage';

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
  const flatBookmarks = React.useMemo(
    () =>
      groups.flatMap((group) => group.items).filter((item) => Boolean(item.app_name)),
    [groups],
  );

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
          <Text style={styles.centerText}>{t('common.loading')}</Text>
        </View>
      </View>
    );
  }

  if (error) {
    return (
      <View style={styles.container}>
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
        <View style={styles.centerContainer}>
          <Text style={styles.emptyText}>{t('actr.bookmark.empty')}</Text>
          <Text style={styles.emptySubText}>{t('actr.bookmark.emptyHint')}</Text>
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
              style={styles.card}
              activeOpacity={0.8}
              onPress={() => openBookmark(item)}
            >
              <View style={styles.cardInner}>
                <Image source={{ uri: item.logo! }} style={styles.cardLogo} />
                <Text style={styles.cardTitle} numberOfLines={1}>
                  {item.app_name}
                </Text>
                <Pressable
                  hitSlop={8}
                  style={styles.removeButton}
                  onPress={(event) => {
                    event.stopPropagation();
                    handleRemoveBookmark(item);
                  }}
                >
                  <Text style={styles.removeButtonText}>×</Text>
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
    paddingHorizontal: 30,
    paddingBottom: 48,
  },
  grid: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    columnGap: 40,
    rowGap: 20,
  },
  card: {
    width: '22.3077%',
  },
  cardInner: {
    height: 56,
    borderRadius: 20,
    backgroundColor: '#3C3C4399',
    flexDirection: 'row',
    alignItems: 'center',
    paddingVertical: 12,
    paddingHorizontal: 13,
  },
  cardLogo: {
    width: 32,
    height: 32,
    borderRadius: 16,
    backgroundColor: '#2a2a2a',
  },
  cardTitle: {
    flex: 1,
    marginLeft: 12,
    color: '#F2F2F7',
    fontSize: 17,
    fontWeight: '500',
  },
  removeButton: {
    width: 20,
    height: 20,
    marginLeft: 8,
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
