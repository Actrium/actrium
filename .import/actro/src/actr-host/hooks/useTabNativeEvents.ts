import React from 'react';
import { TabEvents } from '../../native-bridge/tab/TabEvents';
import { useBookmarkStore } from '../store/useBookmarkStore';
import { useTabIdMapStore } from '../store/useTabIdMapStore';
import { useUnreadStore } from '../store/useUnreadStore';
import { useAuth } from '../../hooks/auth';
import { stopAllVoicePlayback } from '../features/chat/hooks/useReadAloudPlayback';

/**
 * 监听与当前 tab 相关的原生事件：
 * - 进入全屏：清除未读状态
 * - 退出全屏：预留扩展点
 * - 收藏状态变化：写数据库并通知 native
 */
export function useTabNativeEvents(tabId: string) {
  const currentAppId = useTabIdMapStore((state) => state.tabIdMap[tabId]);
  const { checkAuth } = useAuth(currentAppId || '');

  React.useEffect(() => {
    const unsubscribe = TabEvents.onTabEnteredFullscreen(async (event) => {
      if (event.tabId !== tabId) return;

      const nextAppId = useTabIdMapStore.getState().tabIdMap[tabId];
      if (!nextAppId) return;

      console.log('进入 Tab，执行消息更新操作');

      // 更新内存状态
      useUnreadStore.getState().clearUnread(tabId);

      try {
        // 进入全屏时静默预热登录/授权状态，避免首次点击输入框时再等待检测。
        await checkAuth();
      } catch (error) {
        console.error('[useTabNativeEvents] Failed to prefetch auth status:', error);
      }
    });

    const unsubscribe2 = TabEvents.onTabExitedFullscreen(async (event) => {
      if (event.tabId !== tabId) return;

      const nextAppId = useTabIdMapStore.getState().tabIdMap[tabId];
      if (!nextAppId) return;

      console.log('退出 Tab，执行消息更新操作');
      await stopAllVoicePlayback(`tab-exit:${tabId}`);
    });

    const unsubscribe3 = TabEvents.onTabBookmarkChanged(async (event) => {
      if (event.tabId !== tabId) return;

      const currentAppId = useTabIdMapStore.getState().tabIdMap[tabId];
      if (!currentAppId) return;

      // 原生回流同 appId 其他 tab 的同步事件时，
      // store 往往已经处于目标状态；这里直接按当前状态做幂等判断，避免重复写库。
      if (
        useBookmarkStore.getState().isBookmarked(currentAppId) ===
        event.isBookmarked
      ) {
        return;
      }

      console.log('收藏状态变化', event.isBookmarked);

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
