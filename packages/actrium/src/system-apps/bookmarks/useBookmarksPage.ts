import React from 'react';
import { Alert } from 'react-native';
import type { TFunction } from 'i18next';
import {
  TBookmarkGroup,
  TBookmarkItem,
} from '../../repository/BookmarkRepository';
import { openTab } from '../../native-bridge/tab/TabModule';
import { useBookmarkStore } from '../../actr-host/store/useBookmarkStore';
import {
  deleteBrowserBookmark,
  hasBrowserDataModule,
  loadBrowserBookmarks,
} from '../../native-bridge/browser-data/BrowserDataModule';
import type { BookmarkData } from '../../actr-host/types';

type UseBookmarksPageOptions = {
  t: TFunction;
};

export type UseBookmarksPageResult = {
  groups: TBookmarkGroup[];
  isLoading: boolean;
  error: string | null;
  hasBookmarks: boolean;
  loadBookmarks: () => Promise<void>;
  removeBookmark: (item: TBookmarkItem) => Promise<void>;
  openBookmark: (item: TBookmarkItem) => Promise<void>;
};

const BROWSER_BOOKMARK_GROUP_ID = -2;

const stableNativeBookmarkId = (bookmark: BookmarkData, index: number) => {
  const value = bookmark.appURL || bookmark.id || `${bookmark.title}:${index}`;
  let hash = 0x811c9dc5;
  for (let i = 0; i < value.length; i += 1) {
    hash ^= value.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return -Math.abs(hash % 1000000000) - 1;
};

const nativeBookmarkToRuntimeItem = (
  bookmark: BookmarkData,
  index: number,
): TBookmarkItem => ({
  id: stableNativeBookmarkId(bookmark, index),
  source: 'browser',
  category_id: BROWSER_BOOKMARK_GROUP_ID,
  app_id: bookmark.appURL || bookmark.id,
  created_at: bookmark.createdAt,
  sort_order: index,
  app_name: bookmark.title,
  instance_name: bookmark.title,
  app_status: 'alive',
  logo: null,
  url: bookmark.appURL ?? null,
  last_question: null,
  last_answer: null,
});

const nativeBookmarkGroup = (bookmarks: BookmarkData[]): TBookmarkGroup[] => {
  if (bookmarks.length === 0) return [];
  return [
    {
      id: BROWSER_BOOKMARK_GROUP_ID,
      title: 'Browser',
      sort_order: -1,
      items: bookmarks.map(nativeBookmarkToRuntimeItem),
    },
  ];
};

export function useBookmarksPage({
  t,
}: UseBookmarksPageOptions): UseBookmarksPageResult {
  const runtimeGroups = useBookmarkStore((state) => state.groups);
  const isLoading = useBookmarkStore((state) => state.isLoading);
  const loadError = useBookmarkStore((state) => state.error);
  const loadBookmarksFromStore = useBookmarkStore((state) => state.loadBookmarks);
  const removeBookmarkById = useBookmarkStore((state) => state.removeBookmarkById);
  const [nativeGroups, setNativeGroups] = React.useState<TBookmarkGroup[]>([]);
  const nativeBrowserDataAvailable = hasBrowserDataModule();

  const loadBookmarks = React.useCallback(async () => {
    const [, nativeBookmarks] = await Promise.all([
      loadBookmarksFromStore(),
      nativeBrowserDataAvailable ? loadBrowserBookmarks() : Promise.resolve([]),
    ]);
    setNativeGroups(nativeBookmarkGroup(nativeBookmarks));
  }, [loadBookmarksFromStore, nativeBrowserDataAvailable]);

  React.useEffect(() => {
    void loadBookmarks();
  }, [loadBookmarks]);

  const removeBookmark = React.useCallback(
    async (item: TBookmarkItem) => {
      try {
        if (item.source === 'browser') {
          if (await deleteBrowserBookmark(item.url || item.app_id)) {
            await loadBookmarks();
          }
          return;
        }
        await removeBookmarkById(item.id);
      } catch (err) {
        console.error('[BookmarksApp] Failed to remove bookmark:', err);
        Alert.alert(t('common.error'), t('actr.chat.deleteFailed'));
      }
    },
    [loadBookmarks, removeBookmarkById, t],
  );

  const openBookmark = React.useCallback(async (item: TBookmarkItem) => {
    await openTab({
      // Will tabId Always empty, new page sign every time you open a dot.
      // tabId: item.tab_id ?? '',
      appUrl: item.url || item.app_id,
    });
  }, []);

  const groups = React.useMemo(
    () => [...nativeGroups, ...runtimeGroups],
    [nativeGroups, runtimeGroups],
  );
  const hasBookmarks = groups.some((group) => group.items.length > 0);
  const error = loadError ? t('actr.bookmark.loadError') : null;

  return {
    groups,
    isLoading,
    error,
    hasBookmarks,
    loadBookmarks,
    removeBookmark,
    openBookmark,
  };
}
