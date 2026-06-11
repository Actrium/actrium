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
  /** 中文注释：首屏消息加载数量，不同列表实现可使用不同值。 */
  initialMessageLimit: number;
  /**
   * 中文注释：首屏消息加载完成后的回调（无论成功或失败），用于更新 UI 状态
   * （如 hasLoadedInitialMessages、hasMore）。
   * store 的 setMessages 已在 hook 内部处理，回调参数仅为数据引用供外部使用。
   */
  onInitialMessagesLoaded: (data: IMessage[]) => void;
}

/**
 * 中文注释：聊天页初始化生命周期 hook，统一管理四个职责：
 * 1. 加载首屏消息 + 强制最后一条消息完成态
 * 2. App 信息回退（接口失败时从 SQLite 恢复）
 * 3. 同步书签状态
 * 4. 卸载清理（非 DEV 环境下清理 store 数据）
 */
export function useChatInit({
  tab,
  appId,
  initialMessageLimit,
  onInitialMessagesLoaded,
}: UseChatInitParams) {
  // 延后退出选择模式的动画帧 ID，组件卸载时需取消。
  const deferredExitFrameRef = useRef<number | null>(null);

  // 用 ref 持有回调，避免 effect 依赖回调引用导致重复执行。
  const onInitialMessagesLoadedRef = useRef(onInitialMessagesLoaded);
  onInitialMessagesLoadedRef.current = onInitialMessagesLoaded;

  useEffect(() => {
    let isActive = true;

    // ---------- 1. 加载首屏消息 ----------
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

        // 带 appUrl 进入聊天页通常是恢复中的旧会话，初始化后把当前 tab
        // 最后一条消息强制收敛为完成态，避免历史会话残留的 pending/processing 状态在重开后一直卡住。
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

    // ---------- 2. App 信息回退 ----------
    const loadAppInfo = async () => {
      const appInfo = useAppStore.getState().apps[appId];
      if (!appId || appInfo) return;

      const initResult = await AppSessionService.initSession(tab.id, appId);
      if (initResult.ok) return;

      // 带 preset appUrl 进入 Chat 时，优先尊重线上结果；
      // 只有接口失败后，才允许回落到 SQLite 里的旧应用信息继续恢复历史会话。
      if (!tab.appUrl) return;

      const localAppInfo = await AppRepository.getApp(appId);
      if (!localAppInfo) return;

      // DB 里的 url 可能是旧值，恢复旧会话时要优先保留当前 tab 透传的 appUrl，
      // 保证后续分享、tab 信息和宿主恢复语义仍然对应本次打开的地址。
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

    // ---------- 3. 同步书签状态 ----------
    const syncInitialBookmarkStatus = async () => {
      if (!appId) return;

      try {
        // 聊天页只消费书签 store 的内存态；
        // 若当前进程内尚未加载过书签，则先由 store 统一回源 SQLite。
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

    // ---------- 4. 卸载清理 ----------
    return () => {
      isActive = false;

      if (!__DEV__) {
        console.warn(`清理数据 tab: ${tab.id}`);
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
