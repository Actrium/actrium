import { create } from 'zustand';
import { MessageRepository } from '../../repository/MessageRepository';
import { setTabUnreadStatus } from '../../native-bridge/tab/TabModule';

interface UnreadState {
  // tabId -> Number of unread messages
  unreadCounts: Record<string, number>;
  // Total unread messages
  totalUnread: number;

  // Add Assign Tab Unread Number of
  incrementUnread: (tabId: string) => void;

  // & Clear Aspect Tab Unread Number of
  clearUnread: (tabId: string) => void;

  // Batch Settings Unread (for initialization)
  setUnreadCounts: (counts: Record<string, number>) => void;

  // Sync Unread From Database
  syncFromDatabase: () => Promise<void>;
}

export const useUnreadStore = create<UnreadState>((set, get) => ({
  unreadCounts: {},
  totalUnread: 0,

  incrementUnread: (tabId: string) => {
    set((state) => {
      const currentCount = state.unreadCounts[tabId] || 0;
      const newUnreadCounts = {
        ...state.unreadCounts,
        [tabId]: currentCount + 1,
      };
      const newTotalUnread = Object.values(newUnreadCounts).reduce(
        (sum, count) => sum + count,
        0,
      );

      console.log(
        `[UnreadStore] Increment unread for ${tabId}: ${currentCount} -> ${currentCount + 1}`,
      );

      return {
        unreadCounts: newUnreadCounts,
        totalUnread: newTotalUnread,
      };
    });
    // set() The response must be synchronized.async Call Outside fire-and-forget
    setTabUnreadStatus(tabId, true).catch((e) =>
      console.error('[UnreadStore] Failed to notify native increment:', e),
    );
  },

  clearUnread: (tabId: string) => {
    set((state) => {
      const { [tabId]: _, ...restCounts } = state.unreadCounts;
      const newTotalUnread = Object.values(restCounts).reduce(
        (sum, count) => sum + count,
        0,
      );

      console.log(`[UnreadStore] Clear unread for ${tabId}`);

      return {
        unreadCounts: restCounts,
        totalUnread: newTotalUnread,
      };
    });

    // Update database marked as read
    MessageRepository.markAllAsReadByTabId(tabId);
    // Announcements native Update tab Unread State
    setTabUnreadStatus(tabId, false).catch((e) => {
      // console.error('[UnreadStore] Failed to notify native clear:', e),
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
