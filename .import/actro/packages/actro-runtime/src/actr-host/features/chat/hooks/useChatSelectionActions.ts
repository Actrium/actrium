import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { IMessage } from '../../../../repository/MessageRepository';
import { GlobalAlert } from '../../../../components';
import { useMessageStore } from '../../../store/useMessageStore';
import type { ToastRef } from '../../shared/Toast';

interface UseChatSelectionActionsParams {
  tabId: string;
  toastRef: React.RefObject<ToastRef | null>;
}

// 提取删除/分享确认逻辑为 hook，供 ChatLegend 和 Chat 复用。
// Chat.tsx 中这段逻辑散落在组件内部（handleDeletePress / handleShareConfirm / shareMessages state /
// deferredExitSelectionFrameRef），提取后避免逻辑漂移。
export function useChatSelectionActions({
  tabId,
  toastRef,
}: UseChatSelectionActionsParams) {
  const { t } = useTranslation();
  const exitSelectionMode = useMessageStore((state) => state.exitSelectionMode);
  const deleteSelectedMessages = useMessageStore(
    (state) => state.deleteSelectedMessages,
  );

  // 分享预览弹窗的数据源，非空时打开弹窗。
  const [shareMessages, setShareMessages] = useState<IMessage[]>([]);

  // 延后退出选择模式的动画帧 ID，用于取消和清理。
  const deferredExitFrameRef = useRef<number | null>(null);

  // 弹出确认对话框，确认后删除选中消息。
  const handleDeletePress = useCallback(() => {
    GlobalAlert.alert(
      t('actr.chat.deleteConfirmTitle'),
      t('actr.chat.deleteConfirmMsg'),
      [
        {
          text: t('common.cancel'),
          style: 'cancel',
          onPress: () => {
            exitSelectionMode();
          },
        },
        {
          text: t('common.delete'),
          style: 'destructive',
          onPress: () => {
            // setTimeout(0) 避免阻塞 Alert 关闭动画。
            setTimeout(() => {
              deleteSelectedMessages(tabId).catch((error) => {
                console.error(
                  '[useChatSelectionActions] Failed to delete selected messages:',
                  error,
                );
                toastRef.current?.show(t('actr.chat.deleteFailed'));
              });
            }, 0);
          },
        },
      ],
    );
  }, [tabId, deleteSelectedMessages, exitSelectionMode, t, toastRef]);

  // 构建部分消息数组（只包含选中的 part），打开分享预览弹窗，延后退出选择模式。
  const handleShareConfirm = useCallback(() => {
    const byId = useMessageStore.getState().messageById[tabId] || {};
    const orderedIds = useMessageStore.getState().messageIds[tabId] || [];
    const currentSelectedItems = useMessageStore.getState().selectedItems;

    // 按消息在列表中的原始顺序排列，保证分享图内容顺序与聊天一致。
    const orderedMessages: IMessage[] = [];
    for (const id of orderedIds) {
      const parts = currentSelectedItems[id];
      if (!parts || parts.size === 0) continue;
      const message = byId[id];
      if (!message) continue;

      // 根据选中的 part 构建部分消息，只包含用户选中的内容。
      const partial: IMessage = { ...message };
      if (!parts.has('attachments')) partial.attachments = undefined;
      if (!parts.has('question')) {
        partial.question_content = '';
        partial.question_voice_payload = undefined;
      }
      if (!parts.has('answer')) {
        partial.answer_content = '';
        partial.answer_voice_payload = undefined;
      }
      orderedMessages.push(partial);
    }

    setShareMessages(orderedMessages);

    // 延后两帧退出选择模式，避免列表退出动画阻塞分享弹窗首帧渲染。
    if (deferredExitFrameRef.current !== null) {
      cancelAnimationFrame(deferredExitFrameRef.current);
    }
    deferredExitFrameRef.current = requestAnimationFrame(() => {
      deferredExitFrameRef.current = requestAnimationFrame(() => {
        deferredExitFrameRef.current = null;
        exitSelectionMode();
      });
    });
  }, [tabId, exitSelectionMode]);

  // 选中项计数，供 DeleteButton 显示 "删除(N)"。
  const selectedCount = useMessageStore(
    useCallback((state) => {
      let count = 0;
      for (const parts of Object.values(state.selectedItems)) {
        count += parts.size;
      }
      return count;
    }, []),
  );

  // tab 卸载时清理残留的延后退出动画帧，防止内存泄漏。
  useEffect(() => {
    return () => {
      if (deferredExitFrameRef.current !== null) {
        cancelAnimationFrame(deferredExitFrameRef.current);
        deferredExitFrameRef.current = null;
      }
    };
  }, []);

  return {
    handleDeletePress,
    handleShareConfirm,
    shareMessages,
    setShareMessages,
    selectedCount,
  };
}
