import { create } from 'zustand';
import {
  BookmarkRepository,
  TBookmarkGroup,
  TBookmarkItem,
} from '../../repository/BookmarkRepository';
import { setTabBookmarkStatus } from '../../native-bridge/tab/TabModule';
import { useTabIdMapStore } from './useTabIdMapStore';

type AddBookmarkParams = {
  appId: string;
  categoryId?: number | null;
  sortOrder?: number;
};

type SyncNativeBookmarkStatusParams = {
  appId: string;
};

interface BookmarkState {
  groups: TBookmarkGroup[];
  itemsByAppId: Record<string, TBookmarkItem>;
  isLoaded: boolean;
  isLoading: boolean;
  error: Error | null;
  loadBookmarks: (force?: boolean, silent?: boolean) => Promise<TBookmarkGroup[]>;
  addBookmark: (params: AddBookmarkParams) => Promise<void>;
  removeBookmarkById: (id: number) => Promise<void>;
  removeBookmarkByAppId: (appId: string) => Promise<void>;
  getBookmarkByAppId: (appId: string) => TBookmarkItem | null;
  isBookmarked: (appId: string) => boolean;
  getAllBookmarkItems: () => TBookmarkItem[];
  syncNativeBookmarkStatusByAppId: (
    params: SyncNativeBookmarkStatusParams,
  ) => Promise<void>;
}

let loadBookmarksPromise: Promise<TBookmarkGroup[]> | null = null;

const buildItemsByAppId = (groups: TBookmarkGroup[]) => {
  const itemsByAppId: Record<string, TBookmarkItem> = {};

  for (const group of groups) {
    for (const item of group.items) {
      itemsByAppId[item.app_id] = item;
    }
  }

  return itemsByAppId;
};

const isTabNotFoundError = (error: unknown) => {
  if (!(error instanceof Error)) {
    return false;
  }

  return error.message.includes('Tab with ID') && error.message.includes('not found');
};

const applyBookmarks = (
  groups: TBookmarkGroup[],
): Pick<BookmarkState, 'groups' | 'itemsByAppId' | 'isLoaded' | 'isLoading' | 'error'> => ({
  groups,
  itemsByAppId: buildItemsByAppId(groups),
  isLoaded: true,
  isLoading: false,
  error: null,
});

export const useBookmarkStore = create<BookmarkState>((set, get) => ({
  groups: [],
  itemsByAppId: {},
  isLoaded: false,
  isLoading: false,
  error: null,

  loadBookmarks: async (force = false, silent = false) => {
    const state = get();
    if (!force && state.isLoaded) {
      return state.groups;
    }

    if (!force && loadBookmarksPromise) {
      return loadBookmarksPromise;
    }

    // Do not set silently to refresh isLoading, avoids unmounting the list leading to exit animation interruption
    if (!silent) {
      set((prev) => ({
        ...prev,
        isLoading: true,
        error: null,
      }));
    }

    loadBookmarksPromise = BookmarkRepository.getBookmarksHierarchy()
      .then((groups) => {
        set(applyBookmarks(groups));
        return groups;
      })
      .catch((error) => {
        const normalizedError =
          error instanceof Error ? error : new Error('Failed to load bookmarks');
        if (!silent) {
          set((prev) => ({
            ...prev,
            isLoading: false,
            error: normalizedError,
          }));
        }
        throw normalizedError;
      })
      .finally(() => {
        loadBookmarksPromise = null;
      });

    return loadBookmarksPromise;
  },

  addBookmark: async ({ appId, categoryId, sortOrder = 0 }) => {
    await BookmarkRepository.addBookmark({
      app_id: appId,
      category_id: categoryId,
      sort_order: sortOrder,
    });
    await get().loadBookmarks(true);
    await get().syncNativeBookmarkStatusByAppId({ appId });
  },

  removeBookmarkById: async (id) => {
    const bookmark = Object.values(get().itemsByAppId).find((item) => item.id === id);
    await BookmarkRepository.removeBookmarkById(id);
    // Silent refreshment: no isLoading, maintain list mount to support the deletion of transition animations
    await get().loadBookmarks(true, true);
    if (bookmark?.app_id) {
      await get().syncNativeBookmarkStatusByAppId({
        appId: bookmark.app_id,
      });
    }
  },

  removeBookmarkByAppId: async (appId) => {
    await BookmarkRepository.removeBookmarkByAppId(appId);
    // Silent refreshment: no isLoading, maintain list mount to support the deletion of transition animations
    await get().loadBookmarks(true, true);
    await get().syncNativeBookmarkStatusByAppId({ appId });
  },

  getBookmarkByAppId: (appId) => get().itemsByAppId[appId] ?? null,

  isBookmarked: (appId) => Boolean(get().itemsByAppId[appId]),

  getAllBookmarkItems: () => get().groups.flatMap((group) => group.items),

  syncNativeBookmarkStatusByAppId: async ({ appId }) => {
    const isBookmarked = get().isBookmarked(appId);
    const targetTabIds = useTabIdMapStore.getState().getTabIdsByAppId(appId);

    await Promise.all(
      targetTabIds.map(async (tabId) => {
        try {
          await setTabBookmarkStatus(tabId, isBookmarked);
        } catch (error) {
          // Batch Sync Collection State, some tab Could have been closed. native Destroyed,
          // The whole synchronisation should not be considered a failure, but only to clear the residual map and ignore it.
          if (isTabNotFoundError(error)) {
            useTabIdMapStore.getState().removeTabIdMap(tabId);
            return;
          }

          throw error;
        }
      }),
    );
  },
}));
