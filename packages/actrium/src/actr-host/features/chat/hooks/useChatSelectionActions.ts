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

// Ripping Delete/Share confirmation logic as hook♪ For ChatLegend and Chat Re-use.
// Chat.tsx This logic is scattered within the component (handleDeletePress / handleShareConfirm / shareMessages state /
// deferredExitSelectionFrameRef) avoids logical drift after extraction.
export function useChatSelectionActions({
  tabId,
  toastRef,
}: UseChatSelectionActionsParams) {
  const { t } = useTranslation();
  const exitSelectionMode = useMessageStore((state) => state.exitSelectionMode);
  const deleteSelectedMessages = useMessageStore(
    (state) => state.deleteSelectedMessages,
  );

  // Share the data sources for preview windows and open them when they are not empty.
  const [shareMessages, setShareMessages] = useState<IMessage[]>([]);

  // Animation frame for late exit selection mode ID- For cancellation and clean-up.
  const deferredExitFrameRef = useRef<number | null>(null);

  // Pops up confirmation dialogue box, and then deletes the selected message.
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
            // setTimeout(0) Avoid blocking. Alert Close the animation.
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

  // Build Selected Messages Set (includes selected only) part) Opens the shared preview window and then drops out of the selection mode.
  const handleShareConfirm = useCallback(() => {
    const byId = useMessageStore.getState().messageById[tabId] || {};
    const orderedIds = useMessageStore.getState().messageIds[tabId] || [];
    const currentSelectedItems = useMessageStore.getState().selectedItems;

    // Ensures that the message is in the original order of the list, and that the order of the message is the same as the chat.
    const orderedMessages: IMessage[] = [];
    for (const id of orderedIds) {
      const parts = currentSelectedItems[id];
      if (!parts || parts.size === 0) continue;
      const message = byId[id];
      if (!message) continue;

      // By Selected part Builds a partial message that contains only the user selected.
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

    // Delays the two frames to exit selection mode to avoid the list of animating the blockage of sharing the headlight dye of the bullet window.
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

  // Select items to count for DeleteButton Show "Delete(N)".
  const selectedCount = useMessageStore(
    useCallback((state) => {
      let count = 0;
      for (const parts of Object.values(state.selectedItems)) {
        count += parts.size;
      }
      return count;
    }, []),
  );

  // tab Cleans up residual delayed exit animation frames on unloaded to prevent memory leakage.
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
