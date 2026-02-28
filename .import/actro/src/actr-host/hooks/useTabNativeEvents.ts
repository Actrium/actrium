import React from 'react';
import { TabEvents } from '../../native-bridge/tab/TabEvents';
import { setTabBookmarkStatus } from '../../native-bridge/tab/TabModule';
import { useTabIdMapStore } from '../store/useTabIdMapStore';
import { useUnreadStore } from '../store/useUnreadStore';
import { BookmarkRepository } from '../../repository/BookmarkRepository';

/**
 * 监听与当前 tab 相关的原生事件：
 * - 进入全屏：清除未读状态
 * - 退出全屏：预留扩展点
 * - 收藏状态变化：写数据库并通知 native
 */
export function useTabNativeEvents(tabId: string) {
  React.useEffect(() => {
    const unsubscribe = TabEvents.onTabEnteredFullscreen(async (event) => {
      if (event.tabId !== tabId) return;

      const currentAppId = useTabIdMapStore.getState().tabIdMap[tabId];
      if (!currentAppId) return;

      console.log('进入 Tab，执行消息更新操作');

      // 更新内存状态
      useUnreadStore.getState().clearUnread(currentAppId, tabId);
    });

    const unsubscribe2 = TabEvents.onTabExitedFullscreen(async (event) => {
      if (event.tabId !== tabId) return;

      const currentAppId = useTabIdMapStore.getState().tabIdMap[tabId];
      if (!currentAppId) return;

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
          tab_id: event.tabId,
          sort_order: 0,
        });
      } else {
        // 取消收藏，删除数据库
        await BookmarkRepository.removeBookmark(event.tabId);
      }

      // 通知 native 收藏成功
      await setTabBookmarkStatus(event.tabId, event.isBookmarked);
    });

    return () => {
      unsubscribe();
      unsubscribe2();
      unsubscribe3();
    };
  }, [tabId]);
}
