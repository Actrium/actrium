import React from 'react';
import {
  View,
  StyleSheet,
  Image,
  ActivityIndicator,
  TouchableOpacity,
} from 'react-native';
import Animated, { LinearTransition } from 'react-native-reanimated';
import { SwipeableMethods } from 'react-native-gesture-handler/ReanimatedSwipeable';
import { useTranslation } from 'react-i18next';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { AppText, Icon } from '../../components';
import { getTypographyStyle, spacing, controlHeight } from '../../theme/tokens';
import type { TBookmarkItem } from '../../repository/BookmarkRepository';
import { SwipeRemoveActionRow } from '../components/SwipeRemoveActionRow';
import type { UseBookmarksPageResult } from './useBookmarksPage';
import { PhoneNavigationBar } from '../../components/PhoneNavigationBar';
import { BookmarkEmptyState } from './BookmarkEmptyState';

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
  const insets = useSafeAreaInsets();
  const hasNativeNavigationBar = !!PhoneNavigationBar;
  const [, setOpenedSwipeItemId] = React.useState<number | null>(null);
  const openedSwipeItemIdRef = React.useRef<number | null>(null);
  const swipeableRefs = React.useRef<Record<number, SwipeableMethods | null>>(
    {},
  );
  const navigationBarHeight = React.useMemo(
    () => (hasNativeNavigationBar ? insets.top + 66 : 0),
    [insets.top, hasNativeNavigationBar],
  );

  const headerNode = hasNativeNavigationBar ? (
    <PhoneNavigationBar
      title='AskAway'
      showLargeTitle={true}
      style={[styles.headerOverlay, { height: navigationBarHeight }]}
    />
  ) : null;

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
  const getDisplayName = React.useCallback((item: TBookmarkItem) => {
    // dead 状态只作为名字后缀展示，避免改动书签列表的原始排序和点击行为。
    if (item.app_status === 'dead') {
      return `${item.instance_name}（已停止服务）`;
    }
    return item.instance_name;
  }, []);

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

            <AppText style={styles.listItemName} numberOfLines={1}>
              {getDisplayName(item)}
            </AppText>
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
        {headerNode}
        <View style={styles.centerContainer}>
          <ActivityIndicator size='large' color='#34A8FB' />
          <AppText style={styles.centerText}>{t('common.loading')}</AppText>
        </View>
      </View>
    );
  }

  if (error) {
    return (
      <View style={styles.container}>
        {headerNode}
        <View style={styles.centerContainer}>
          <AppText style={styles.errorText}>{error}</AppText>
          <TouchableOpacity style={styles.retryButton} onPress={loadBookmarks}>
            <AppText style={styles.retryButtonText}>
              {t('common.retry')}
            </AppText>
          </TouchableOpacity>
        </View>
      </View>
    );
  }

  if (!hasBookmarks) {
    return (
      <View style={styles.container}>
        {headerNode}
        <BookmarkEmptyState />
      </View>
    );
  }

  return (
    <View style={styles.container}>
      {headerNode}

      <Animated.FlatList
        itemLayoutAnimation={LinearTransition}
        style={styles.content}
        contentContainerStyle={[
          styles.scrollContent,
          hasNativeNavigationBar ? { paddingTop: navigationBarHeight } : null,
        ]}
        showsVerticalScrollIndicator={false}
        scrollIndicatorInsets={
          hasNativeNavigationBar ? { top: navigationBarHeight } : undefined
        }
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
  headerOverlay: {
    position: 'absolute',
    top: 0,
    left: 0,
    right: 0,
    width: '100%',
    zIndex: 10,
  },
  content: {
    flex: 1,
  },
  scrollContent: {
    paddingHorizontal: spacing.lg,
    paddingBottom: 40,
  },
  listItemWrapper: {
    backgroundColor: '#1a1a1a',
    overflow: 'hidden',
  },
  listItemGroupTop: {
    borderTopLeftRadius: 26,
    borderTopRightRadius: 26,
    marginTop: 24,
  },
  listItemGroupBottom: {
    borderBottomLeftRadius: 26,
    borderBottomRightRadius: 26,
  },
  listItem: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: spacing.lg,
    height: controlHeight.xl,
    backgroundColor: '#1a1a1a',
  },
  listItemDivider: {
    height: 1,
    backgroundColor: '#2C2C2E',
    // 分隔线起点跟 settings 菜单项保持一致，需要避开左侧图标和文本起始前的间距。
    marginLeft: 55,
    marginRight: spacing.md,
  },
  listItemLogo: {
    width: 22,
    height: 22,
    borderRadius: 11,
    backgroundColor: '#2a2a2a',
  },
  listItemName: {
    flex: 1,
    ...getTypographyStyle('title3'),
    color: '#fff',
    marginLeft: 17,
    fontWeight: '400',
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
