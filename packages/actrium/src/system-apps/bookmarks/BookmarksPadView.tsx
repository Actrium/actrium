import React from 'react';
import {
  View,
  ScrollView,
  StyleSheet,
  Image,
  ActivityIndicator,
  TouchableOpacity,
  useWindowDimensions,
} from 'react-native';
import Animated, { LinearTransition, FadeOut } from 'react-native-reanimated';
import Clipboard from '@react-native-clipboard/clipboard';
import { useTranslation } from 'react-i18next';
import { AppText, Icon, GlobalAlertModal } from '../../components';
import {
  DropdownMenu,
  DropdownMenuItem,
  DropdownMenuText,
} from '../../actr-host/features/shared/DropdownMenu';
import { Toast, type ToastRef } from '../../actr-host/features/shared/Toast';
import { getTypographyStyle, spacing, radius, longPressMenu } from '../../theme/tokens';
import type { TBookmarkItem } from '../../repository/BookmarkRepository';
import type { UseBookmarksPageResult } from './useBookmarksPage';
import { BookmarkEmptyState } from './BookmarkEmptyState';

// Pad End Fixed Presentation 4 Columns, but width must be calculated in real available space and no dead design percentage can be written.
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
  const toastRef = React.useRef<ToastRef>(null);
  const [pendingDeleteItem, setPendingDeleteItem] = React.useState<TBookmarkItem | null>(null);
  const flatBookmarks = React.useMemo(
    () =>
      groups.flatMap((group) => group.items).filter((item) => Boolean(item.instance_name)),
    [groups],
  );
  const getDisplayName = React.useCallback((item: TBookmarkItem) => {
    // When application is marked as dead , only add a state suffix to the name, without prejudice to the original name display.
    if (item.app_status === 'dead') {
      return `${item.instance_name}(services discontinued)`;
    }
    return item.instance_name;
  }, []);
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

  const handleCopyUrl = React.useCallback(
    (item: TBookmarkItem) => {
      try {
        Clipboard.setString(item.url || item.app_id);
        toastRef.current?.show(t('actr.bookmark.copySuccess'));
      } catch (e) {
        console.error('Failed to copy URL', e);
        toastRef.current?.show(t('actr.bookmark.copyFailed'));
      }
    },
    [t],
  );

  const handleOpenBookmark = React.useCallback(
    (item: TBookmarkItem) => {
      openBookmark(item);
    },
    [openBookmark],
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
        <BookmarkEmptyState />
      </View>
    );
  }

  return (
    <View style={styles.container}>
      <Toast ref={toastRef} />
      <GlobalAlertModal
        visible={pendingDeleteItem !== null}
        title={t('actr.bookmark.removeTitle')}
        message={t('actr.bookmark.removeMessage')}
        buttons={[
          { text: t('common.cancel'), style: 'cancel' },
          { text: t('actr.bookmark.remove'), style: 'destructive', onPress: () => handleRemoveBookmark(pendingDeleteItem!) },
        ]}
        options={{ cancelable: true }}
        onClose={() => setPendingDeleteItem(null)}
      />
      <ScrollView
        style={styles.content}
        contentContainerStyle={styles.scrollContent}
        showsVerticalScrollIndicator={false}
      >
        <View style={styles.grid}>
          {flatBookmarks.map((item) => (
            <Animated.View
              key={item.id}
              layout={LinearTransition}
              exiting={FadeOut.duration(200)}
              style={[styles.card, { width: cardWidth }]}
            >
              <DropdownMenu
                width={longPressMenu.width}
                estimatedWidth={longPressMenu.width}
                anchorMode='end'
                onTriggerPress={() => handleOpenBookmark(item)}
                trigger={
                  <View style={styles.cardInner}>
                    {item.logo ? (
                      <Image source={{ uri: item.logo }} style={styles.cardLogo} />
                    ) : (
                      <View style={[styles.cardLogo, styles.cardLogoFallback]}>
                        <Icon type='link' size={18} color='#8E8E93' />
                      </View>
                    )}
                    <AppText style={styles.cardTitle} numberOfLines={1}>
                      {getDisplayName(item)}
                    </AppText>
                  </View>
                }
                triggerType='longPress'
              >
                {(close) => (
                  <>
                    <DropdownMenuItem
                      onPress={() => {
                        close(() => handleOpenBookmark(item));
                      }}
                    >
                      <Icon type='enter' size={16} />
                      <DropdownMenuText>{t('actr.bookmark.openInNewTab')}</DropdownMenuText>
                    </DropdownMenuItem>
                    <DropdownMenuItem
                      onPress={() => {
                        handleCopyUrl(item);
                        close();
                      }}
                    >
                      <Icon type='copy' size={16} />
                      <DropdownMenuText>{t('actr.bookmark.copyUrl')}</DropdownMenuText>
                    </DropdownMenuItem>
                    <DropdownMenuItem
                      onPress={() => {
                        close(() => setPendingDeleteItem(item));
                      }}
                    >
                      <Icon type='delete' size={16} color='#FF3B30' />
                      <DropdownMenuText color='#FF3B30'>{t('actr.bookmark.remove')}</DropdownMenuText>
                    </DropdownMenuItem>
                  </>
                )}
              </DropdownMenu>
            </Animated.View>
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
  },
  cardLogo: {
    width: 32,
    height: 32,
    borderRadius: 16,
    backgroundColor: '#2a2a2a',
  },
  cardLogoFallback: {
    alignItems: 'center',
    justifyContent: 'center',
  },
  cardTitle: {
    flex: 1,
    marginLeft: spacing.md,
    color: '#F2F2F7',
    ...getTypographyStyle('title3'),
    fontWeight: '500',
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
});
