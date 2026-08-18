import React from 'react';
import {
  ActivityIndicator,
  FlatList,
  Image,
  StyleSheet,
  TouchableOpacity,
  View,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { AppText, Icon } from '../../components';
import type { TBookmarkItem } from '../../repository/BookmarkRepository';
import { getTypographyStyle, radius, spacing } from '../../theme/tokens';
import { BookmarkEmptyState } from './BookmarkEmptyState';
import { useBookmarksPage } from './useBookmarksPage';

type FlatBookmark = TBookmarkItem & {
  groupTitle: string;
};

/**
 * macOS Renderer variant.
 *
 * The shared mobile views depend on Reanimated/GestureHandler native parts.
 * Those are not part of the thin Renderer Helper, so keep the same load/open/
 * delete business flow with native React components and no animation runtime.
 */
export default function BookmarksApp() {
  const { t } = useTranslation();
  const {
    groups,
    isLoading,
    error,
    hasBookmarks,
    loadBookmarks,
    removeBookmark,
    openBookmark,
  } = useBookmarksPage({ t });

  const items = React.useMemo<FlatBookmark[]>(
    () =>
      groups.flatMap((group) =>
        group.items.map((item) => ({
          ...item,
          groupTitle: group.title,
        })),
      ),
    [groups],
  );

  if (isLoading) {
    return (
      <View style={styles.center}>
        <ActivityIndicator size='large' color='#34A8FB' />
        <AppText style={styles.statusText}>{t('common.loading')}</AppText>
      </View>
    );
  }

  if (error) {
    return (
      <View style={styles.center}>
        <AppText style={styles.errorText}>{error}</AppText>
        <TouchableOpacity style={styles.retryButton} onPress={loadBookmarks}>
          <AppText style={styles.retryText}>{t('common.retry')}</AppText>
        </TouchableOpacity>
      </View>
    );
  }

  if (!hasBookmarks) {
    return (
      <View style={styles.container}>
        <BookmarkEmptyState />
      </View>
    );
  }

  return (
    <View style={styles.container}>
      <FlatList
        data={items}
        keyExtractor={(item) => `${item.source ?? 'runtime'}:${item.id}`}
        contentContainerStyle={styles.list}
        renderItem={({ item, index }) => {
          const showGroup =
            index === 0 || items[index - 1]?.groupTitle !== item.groupTitle;
          const title =
            item.instance_name || item.app_name || item.url || item.app_id;
          return (
            <View>
              {showGroup ? (
                <AppText style={styles.groupTitle}>{item.groupTitle}</AppText>
              ) : null}
              <View style={styles.row}>
                <TouchableOpacity
                  style={styles.openButton}
                  onPress={() => void openBookmark(item)}
                >
                  {item.logo ? (
                    <Image source={{ uri: item.logo }} style={styles.logo} />
                  ) : (
                    <View style={[styles.logo, styles.logoFallback]}>
                      <Icon type='link' size={15} color='#8E8E93' />
                    </View>
                  )}
                  <AppText style={styles.title} numberOfLines={1}>
                    {item.app_status === 'dead'
                      ? `${title}（已停止服务）`
                      : title}
                  </AppText>
                </TouchableOpacity>
                <TouchableOpacity
                  accessibilityLabel={t('actr.bookmark.remove')}
                  style={styles.removeButton}
                  onPress={() => void removeBookmark(item)}
                >
                  <Icon type='delete' size={15} color='#FF453A' />
                </TouchableOpacity>
              </View>
            </View>
          );
        }}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#000',
  },
  center: {
    flex: 1,
    backgroundColor: '#000',
    justifyContent: 'center',
    alignItems: 'center',
    padding: spacing.xl,
  },
  statusText: {
    marginTop: spacing.md,
    color: '#8E8E93',
    ...getTypographyStyle('subhead'),
  },
  errorText: {
    color: '#FF453A',
    marginBottom: spacing.lg,
    ...getTypographyStyle('subhead'),
  },
  retryButton: {
    borderRadius: radius.xl,
    backgroundColor: '#34A8FB',
    paddingHorizontal: spacing.xl,
    paddingVertical: spacing.sm,
  },
  retryText: {
    color: '#fff',
    ...getTypographyStyle('subhead'),
  },
  list: {
    padding: spacing.xl,
  },
  groupTitle: {
    color: '#8E8E93',
    marginTop: spacing.lg,
    marginBottom: spacing.sm,
    ...getTypographyStyle('caption1'),
  },
  row: {
    minHeight: 54,
    borderRadius: radius.lg,
    backgroundColor: '#1a1a1a',
    flexDirection: 'row',
    alignItems: 'center',
    marginBottom: 1,
  },
  openButton: {
    flex: 1,
    minWidth: 0,
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: spacing.lg,
    paddingVertical: spacing.md,
  },
  logo: {
    width: 24,
    height: 24,
    borderRadius: 12,
    backgroundColor: '#2a2a2a',
  },
  logoFallback: {
    alignItems: 'center',
    justifyContent: 'center',
  },
  title: {
    flex: 1,
    color: '#fff',
    marginLeft: spacing.md,
    ...getTypographyStyle('body'),
  },
  removeButton: {
    width: 44,
    height: 44,
    alignItems: 'center',
    justifyContent: 'center',
  },
});
