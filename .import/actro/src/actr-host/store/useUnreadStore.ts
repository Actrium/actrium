import { create } from 'zustand';
import { MessageRepository } from '../../repository/MessageRepository';

interface UnreadState {
  // appId -> 未读消息数量
  unreadCounts: Record<string, number>;
  // 总未读消息数量
  totalUnread: number;

  // 增加指定应用的未读数
  incrementUnread: (appId: string) => void;

  // 清空指定应用的未读数
  clearUnread: (appId: string) => void;

  // 批量设置未读数（用于初始化）
  setUnreadCounts: (counts: Record<string, number>) => void;

  // 从数据库同步未读数
  syncFromDatabase: () => Promise<void>;
}

export const useUnreadStore = create<UnreadState>((set, get) => ({
  unreadCounts: {},
  totalUnread: 0,

  incrementUnread: (appId: string) => {
    set((state) => {
      const currentCount = state.unreadCounts[appId] || 0;
      const newUnreadCounts = {
        ...state.unreadCounts,
        [appId]: currentCount + 1,
      };
      const newTotalUnread = Object.values(newUnreadCounts).reduce(
        (sum, count) => sum + count,
        0,
      );

      console.log(
        `[UnreadStore] Increment unread for ${appId}: ${currentCount} -> ${currentCount + 1}`,
      );

      return {
        unreadCounts: newUnreadCounts,
        totalUnread: newTotalUnread,
      };
    });
  },

  clearUnread: (appId: string) => {
    set((state) => {
      const { [appId]: _, ...restCounts } = state.unreadCounts;
      const newTotalUnread = Object.values(restCounts).reduce(
        (sum, count) => sum + count,
        0,
      );

      console.log(`[UnreadStore] Clear unread for ${appId}`);

      return {
        unreadCounts: restCounts,
        totalUnread: newTotalUnread,
      };
    });
  },

  setUnreadCounts: (counts: Record<string, number>) => {
    const totalUnread = Object.values(counts).reduce(
      (sum, count) => sum + count,
      0,
    );

    console.log('[UnreadStore] Set unread counts:', counts);

    set({
      unreadCounts: counts,
      totalUnread,
    });
  },

  syncFromDatabase: async () => {
    try {
      console.log('[UnreadStore] Syncing from database...');
      const counts = await MessageRepository.getAllUnreadCounts();
      get().setUnreadCounts(counts);
      console.log('[UnreadStore] Sync completed:', counts);
    } catch (error) {
      console.error('[UnreadStore] Failed to sync from database:', error);
    }
  },
}));
