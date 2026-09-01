import React from 'react';
import { TabEvents } from '../../native-bridge/tab/TabEvents';
import { useBookmarkStore } from '../store/useBookmarkStore';
import { useTabIdMapStore } from '../store/useTabIdMapStore';
import { useUnreadStore } from '../store/useUnreadStore';
import { useAuth } from '../../hooks/auth';
import { stopAllVoicePlayback } from '../features/chat/hooks/useReadAloudPlayback';

/**
 * Listening and Current tab Related primary events:
 * - Fullscreen: clear unread state
 * - Exit fullscreen: preset extension points
 * - Collection status change: Write database and notify native
 */
export function useTabNativeEvents(tabId: string) {
  const currentAppId = useTabIdMapStore((state) => state.tabIdMap[tabId]);
  const { checkAuth } = useAuth(currentAppId || '');

  React.useEffect(() => {
    const unsubscribe = TabEvents.onTabEnteredFullscreen(async (event) => {
      if (event.tabId !== tabId) return;

      const nextAppId = useTabIdMapStore.getState().tabIdMap[tabId];
      if (!nextAppId) return;

      console.log('Enter Tab,Perform message update');

      // Update Memory Status
      useUnreadStore.getState().clearUnread(tabId);

      try {
        // Silent pre-heat login when entering full screen/is authorized to avoid waiting for the test when you click the input box for the first time.
        await checkAuth();
      } catch (error) {
        console.error('[useTabNativeEvents] Failed to prefetch auth status:', error);
      }
    });

    const unsubscribe2 = TabEvents.onTabExitedFullscreen(async (event) => {
      if (event.tabId !== tabId) return;

      const nextAppId = useTabIdMapStore.getState().tabIdMap[tabId];
      if (!nextAppId) return;

      console.log('Exit Tab,Perform message update');
      await stopAllVoicePlayback(`tab-exit:${tabId}`);
    });

    const unsubscribe3 = TabEvents.onTabBookmarkChanged(async (event) => {
      if (event.tabId !== tabId) return;

      const currentAppId = useTabIdMapStore.getState().tabIdMap[tabId];
      if (!currentAppId) return;

      // The same old life. appId Other tab Other Organiser
      // store Often, they are already in target status; they are based on the current state of affairs, etc., and avoid duplication of the database.
      if (
        useBookmarkStore.getState().isBookmarked(currentAppId) ===
        event.isBookmarked
      ) {
        return;
      }

      console.log('Collection status changes', event.isBookmarked);

      if (event.isBookmarked) {
        await useBookmarkStore.getState().addBookmark({
          appId: currentAppId,
          sortOrder: 0,
        });
      } else {
        await useBookmarkStore.getState().removeBookmarkByAppId(currentAppId);
      }
    });

    return () => {
      unsubscribe();
      unsubscribe2();
      unsubscribe3();
    };
  }, [checkAuth, tabId]);
}
