import React from 'react';
import { TabEvents } from '../../native-bridge/tab/TabEvents';
import { setTabBookmarkStatus } from '../../native-bridge/tab/TabModule';
import { useTabIdMapStore } from '../store/useTabIdMapStore';
import { useUnreadStore } from '../store/useUnreadStore';
import { BookmarkRepository } from '../../repository/BookmarkRepository';
import { useAuth } from '../../hooks/auth';

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
    });

    const unsubscribe3 = TabEvents.onTabBookmarkChanged(async (event) => {
      if (event.tabId !== tabId) return;

      const currentAppId = useTabIdMapStore.getState().tabIdMap[tabId];
      if (!currentAppId) return;

      console.log('收藏状态变化', event.isBookmarked);

      if (event.isBookmarked) {
        // 收藏，更新数据库
        await BookmarkRepository.addBookmark({
          app_id: currentAppId,
          sort_order: 0,
        });
      } else {
        // 取消收藏，按 app_id 删除
        await BookmarkRepository.removeBookmarkByAppId(currentAppId);
      }

      // 通知所有同 appId 的 tab 更新收藏状态
      const allTabIds = useTabIdMapStore.getState().getTabIdsByAppId(currentAppId);
      await Promise.all(
        allTabIds.map((id) => setTabBookmarkStatus(id, event.isBookmarked)),
      );
    });

    return () => {
      unsubscribe();
      unsubscribe2();
      unsubscribe3();
    };
  }, [checkAuth, tabId]);
}
