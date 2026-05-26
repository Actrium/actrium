import { useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import Clipboard from '@react-native-clipboard/clipboard';
import { useInputStore } from '../../../store/useInputStore';
import { useAppStore } from '../../../store/useAppStore';
import { GlobalAlert } from '../../../../components';
import { IMessage } from '../../../../repository/MessageRepository';
import { IAttachmentItem } from '../../../types/attachment';
import type { ToastRef } from '../../shared/Toast';

interface UseChatBubbleActionsParams {
  tabId: string;
  appId: string;
  toastRef: React.RefObject<ToastRef | null>;
  setFeedbackMessageId: (id: string | null) => void;
}

export function useChatBubbleActions({
  tabId,
  appId,
  toastRef,
  setFeedbackMessageId,
}: UseChatBubbleActionsParams) {
  const { t } = useTranslation();
  const setStoreText = useInputStore((state) => state.setInputText);
  const setStoreAttachments = useInputStore((state) => state.setAttachments);
  const setEditingMessageId = useInputStore(
    (state) => state.setEditingMessageId,
  );
  const clearInput = useInputStore((state) => state.clearInput);
  const appInfo = useAppStore((state) => state.apps[appId]);

  // 中文注释：编辑消息，将文本和附件回填到输入框。
  const handleEdit = useCallback(
    (msg: IMessage) => {
      setStoreText(tabId, msg.question_content || '');
      setEditingMessageId(tabId, msg.message_id);

      if (msg.attachments) {
        try {
          const parsed = JSON.parse(msg.attachments);
          const mapped: IAttachmentItem[] = parsed.map((att: any) => ({
            id: Date.now().toString() + Math.random(),
            attachment_type: att.attachment_type,
            local_url: att.local_url,
            cdn_url: att.cdn_url,
            file_type: att.file_type,
            file_name: att.file_name,
            aspect_ratio: att.aspect_ratio,
            latitude: att.latitude,
            longitude: att.longitude,
          }));
          setStoreAttachments(tabId, mapped);
        } catch (e) {
          console.error('[useChatBubbleActions] Failed to parse attachments for edit', e);
        }
      } else {
        setStoreAttachments(tabId, []);
      }
    },
    [tabId, setStoreText, setStoreAttachments, setEditingMessageId],
  );

  // 中文注释：复制文本到剪贴板并弹出 Toast 提示。
  const handleCopy = useCallback(
    (text: string) => {
      if (!text) return;
      try {
        Clipboard.setString(text);
        toastRef.current?.show(t('actr.chat.copySuccess'));
      } catch (e) {
        console.error('[useChatBubbleActions] Failed to copy to clipboard', e);
        toastRef.current?.show(t('actr.chat.copyFailed'));
      }
    },
    [t, toastRef],
  );

  // 中文注释：取消编辑，一次性清空文本、附件和编辑 ID。
  const handleCancelEdit = useCallback(() => {
    clearInput(tabId);
  }, [tabId, clearInput]);

  // 中文注释：打开反馈弹窗；app 非活跃状态时弹全局提示。
  const handleFeedback = useCallback(
    (msgId: string) => {
      if (appInfo?.app_status !== 'alive') {
        GlobalAlert.alert(t('actr.app.notExist'));
        return;
      }
      setFeedbackMessageId(msgId);
    },
    [appInfo?.app_status, setFeedbackMessageId, t],
  );

  return { handleEdit, handleCopy, handleCancelEdit, handleFeedback };
}
