import { useEffect, useRef } from 'react';
import { IMessage, MessageRepository, Progress } from '../../../../repository/MessageRepository';
import { AppSessionService } from '../../../../services/AppSessionService';
import { AppRepository } from '../../../../repository/AppRepository';
import { useMessageStore } from '../../../store/useMessageStore';
import { useAppStore } from '../../../store/useAppStore';
import { useTabIdMapStore } from '../../../store/useTabIdMapStore';
import { useInputStore } from '../../../store/useInputStore';
import { useUnreadStore } from '../../../store/useUnreadStore';
import { useBookmarkStore } from '../../../store/useBookmarkStore';
import { updateTabInfo } from '../../../../native-bridge/tab/TabModule';
import type { TabConfig } from '../../../types';

interface UseChatInitParams {
  tab: TabConfig;
  appId: string;
  /** Comment in Chinese: The number of front-screen messages loaded is different for different lists. */
  initialMessageLimit: number;
  /**
   * Chinese Comment: First Screen Message Loads Backup after Completion (whether successful or unsuccessful) for Update UI Status
   * (if hasLoadedInitialMessages, hasMore).
   * store It's... setMessages Already hook Internal processing, and the echo parameters are only for data references for external use.
   */
  onInitialMessagesLoaded: (data: IMessage[]) => void;
}

/**
 * Chinese Comment: Initializing the life cycle of chat pages hook• Integrated management of four functions:
 * 1. Loading First Screen Message + Force last message to finish the status
 * 2. App Message Back (from the interface if failed) SQLite (Recovered)
 * 3. Synchronize Bookmark Status
 * 4. Unmount Cleanup (N) DEV Clear Environment store Data)
 */
export function useChatInit({
  tab,
  appId,
  initialMessageLimit,
  onInitialMessagesLoaded,
}: UseChatInitParams) {
  // Animation frame for late exit selection mode ID, the component has to be cancelled when offloaded.
  const deferredExitFrameRef = useRef<number | null>(null);

  // Use ref Hold a callback, avoid it. effect Revert references are relied upon to lead to duplicates.
  const onInitialMessagesLoadedRef = useRef(onInitialMessagesLoaded);
  onInitialMessagesLoadedRef.current = onInitialMessagesLoaded;

  useEffect(() => {
    let isActive = true;

    // ---------- 1. Loading First Screen Message ----------
    const loadInitialMessages = async () => {
      const { setMessages, updateMessage } = useMessageStore.getState();

      try {
        const data = await MessageRepository.getAllMessages(
          tab.id,
          initialMessageLimit,
          0,
        );

        if (!isActive) return;

        setMessages(tab.id, data);

        // - Yeah. appUrl Entering a chat page is usually an old session in recovery, initializing the current tab
        // The last message forced a complete state of affairs to avoid the remnants of history. pending/processing The state is stuck after the re-opening.
        if (tab.appUrl && data.length > 0) {
          const latestMessage = data.reduce((latest, current) => {
            if (current.question_time !== latest.question_time) {
              return current.question_time > latest.question_time
                ? current
                : latest;
            }
            return current.message_id.localeCompare(latest.message_id) > 0
              ? current
              : latest;
          });

          if (latestMessage.progress !== Progress.completed) {
            updateMessage(
              tab.id,
              latestMessage.message_id,
              { progress: Progress.completed },
              true,
            );
          }
        }

        onInitialMessagesLoadedRef.current(data);
      } catch (error) {
        if (isActive) {
          setMessages(tab.id, []);
          onInitialMessagesLoadedRef.current([]);
        }
      }
    };

    // ---------- 2. App Message Back ----------
    const loadAppInfo = async () => {
      const appInfo = useAppStore.getState().apps[appId];
      if (!appId || appInfo) return;

      const initResult = await AppSessionService.initSession(tab.id, appId);
      if (initResult.ok) return;

      // - Yeah. preset appUrl Enter Chat — Priority is given to respecting the results on the line;
      // Only when the interface fails will it be allowed to return to SQLite The old application information in it continues to restore the history session.
      if (!tab.appUrl) return;

      const localAppInfo = await AppRepository.getApp(appId);
      if (!localAppInfo) return;

      // DB - Yes. url Could be old, keep the current first when the old session is restored tab It's over the news. appUrl,
      // ♪ Promised to follow-up sharing,tab The message and host re-enactment semantics still correspond to the address opened this time.
      const resolvedAppInfo = {
        ...localAppInfo,
        url: tab.appUrl || localAppInfo.url,
      };

      useAppStore.getState().setAppInfo(appId, resolvedAppInfo);
      void updateTabInfo({
        tabId: tab.id,
        title: resolvedAppInfo.instance_name,
        icon: resolvedAppInfo.info?.logo || '',
        appUrl: resolvedAppInfo.url || '',
        status: resolvedAppInfo.app_status,
      });
    };

    // ---------- 3. Synchronize Bookmark Status ----------
    const syncInitialBookmarkStatus = async () => {
      if (!appId) return;

      try {
        // Chat Pages Only Bookmarks store ;
        // If not already loaded with bookmarks in the current process, first store Ununified Return Source SQLite.
        await useBookmarkStore.getState().loadBookmarks();
        await useBookmarkStore.getState().syncNativeBookmarkStatusByAppId({
          appId,
        });
      } catch (error) {
        console.error('[useChatInit] Failed to sync initial bookmark status:', error);
      }
    };

    void loadInitialMessages();
    void loadAppInfo();
    void syncInitialBookmarkStatus();

    // ---------- 4. Unmount Cleanup ----------
    return () => {
      isActive = false;

      if (!__DEV__) {
        console.warn(`Clear Data tab: ${tab.id}`);
        useMessageStore.getState().cleanMessage(tab.id);
        useTabIdMapStore.getState().removeTabIdMap(tab.id);
        useUnreadStore.getState().clearUnread(tab.id);
        useInputStore.getState().clearInput(tab.id);
      }

      if (deferredExitFrameRef.current !== null) {
        cancelAnimationFrame(deferredExitFrameRef.current);
        deferredExitFrameRef.current = null;
      }
    };
  }, [tab.id, appId, initialMessageLimit]);

  return { deferredExitFrameRef };
}
