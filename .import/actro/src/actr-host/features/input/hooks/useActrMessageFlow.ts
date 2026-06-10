import React, { useCallback, useEffect, useRef, useState } from 'react';
import { GlobalAlert } from '../../../../components';
import { useTranslation } from 'react-i18next';
import { useMessageStore } from '../../../store/useMessageStore';
import { useAppStore } from '../../../store/useAppStore';
import { useInputStore } from '../../../store/useInputStore';
import type { IMessage } from '../../../../repository/MessageRepository';
import { useActrConnection } from '../../../../hooks/useActrConnection';
import type { TActrMessageResponse, TLocation } from '../../../../libs/actr';
import { useAuth, type AuthStatus } from '../../../../hooks/auth';
import { Utils } from '../../../../utils/utils';
import { useUnreadStore } from '../../../store/useUnreadStore';
import { getNetworkStatusForSend } from '../../../hooks/networkStatus';
import { getCurrentFullscreenTab } from '../../../../native-bridge/tab/TabModule';
import {
  cleanupVoiceMessageFile,
  saveVoiceMessageWav,
} from '../../../utils/VoiceMessageStorage';
import type { IAttachmentItem } from '../../../types/attachment';
import {
  triggerHaptic,
  triggerLongVibration,
} from '../../../../../src/actr-host/utils/haptics';

interface UseActrMessageFlowParams {
  appId: string;
  tabId: string;
  inputText: string;
  editingMessageId: string | null;
  attachments: IAttachmentItem[];
  showTopToast?: (message: string) => void;
}

interface AnswerVoiceBufferState {
  nextSequence: number;
  pendingBySequence: Record<number, Uint8Array>;
  orderedChunks: Uint8Array[];
  receivedSequences: number[];
  endSequence?: number;
}

interface AnswerTextBufferState {
  pendingText: string;
  flushTimer: ReturnType<typeof setTimeout> | null;
  hasTriggeredStartHaptic: boolean;
}

// 中文注释：文本流式分片在这个时间窗内合并后再写入 store，避免每个 chunk 都触发整段聊天区重渲染。
const TEXT_MESSAGE_FLUSH_DELAY_MS = 48;

interface UseActrMessageFlowResult {
  showLoginModal: boolean;
  authParams: {
    clientId: string;
    appId: string;
  } | null;
  showAuthorizeModal: boolean;
  /** 输入框聚焦前调用，未授权时弹出登录/授权弹窗并返回 false */
  checkInputAuth: () => Promise<boolean>;
  processMessageSend: (
    content: string,
    options?: { isVoice?: boolean },
  ) => Promise<any>;
  handleSend: () => void;
  handleLoginModalClose: () => void;
  handleLoginSuccess: () => void;
  handleAuthorizeClose: () => void;
  handleAuthorizeSuccess: () => void;
  sendVoiceChunk: (chunk: Uint8Array) => Promise<unknown>;
  startVoice: (payload: any) => Promise<string>;
  endVoice: () => Promise<unknown>;
  voiceMessageCallbacks: {
    onVoiceMessageReady: (payload: {
      messageId: string;
      message: IMessage;
      voicePayload: string;
    }) => void;
    onVoiceMessageCanceled: (messageId: string) => void;
  };
}

export function useActrMessageFlow(
  params: UseActrMessageFlowParams,
): UseActrMessageFlowResult {
  const {
    appId,
    tabId,
    inputText,
    editingMessageId,
    attachments,
    showTopToast,
  } = params;
  const { t } = useTranslation();
  const appInfo = useAppStore((state) => state.apps[appId]);
  const { checkAuth } = useAuth(appId);

  const clearInputInStore = useInputStore((state) => state.clearInput);
  const setEditingMessageId = useInputStore(
    (state) => state.setEditingMessageId,
  );
  const addMessage = useMessageStore((state) => state.addMessage);
  const updateMessage = useMessageStore((state) => state.updateMessage);
  const deleteMessageFromStore = useMessageStore(
    (state) => state.deleteMessage,
  );

  const [showLoginModal, setShowLoginModal] = useState(false);
  const [authParams, setAuthParams] = useState<{
    clientId: string;
    appId: string;
  } | null>(null);
  const [showAuthorizeModal, setShowAuthorizeModal] = useState(false);

  const pendingVoiceMessageUpdatesRef = useRef<
    Record<string, Partial<IMessage>>
  >({});
  const answerVoiceBuffersRef = useRef<Record<string, AnswerVoiceBufferState>>(
    {},
  );
  // 中文注释：为每条文本回答维护独立缓冲区和定时器，把高频流式分片合并成低频 UI 刷新。
  const answerTextBuffersRef = useRef<Record<string, AnswerTextBufferState>>(
    {},
  );

  const handleAuthStatus = useCallback(
    (status: AuthStatus, clientId: string): boolean => {
      if (status === 'authorized') {
        return true;
      }

      if (status === 'not_logged_in') {
        // 中文注释：Pad 和 Phone 统一使用当前页登录弹窗，不再跳转登录 tab。
        setAuthParams({ clientId, appId });
        setShowLoginModal(true);
        return false;
      }

      if (status === 'not_authorized') {
        setShowAuthorizeModal(true);
        return false;
      }

      return false;
    },
    [appId],
  );

  /**
   * 输入框聚焦前的授权预检。
   * 未授权时弹出登录/授权弹窗并返回 false，已授权返回 true。
   */
  const checkInputAuth = useCallback(async (): Promise<boolean> => {
    const { status, clientId } = await checkAuth();
    return handleAuthStatus(status, clientId);
  }, [checkAuth, handleAuthStatus]);

  // 中文注释：清理指定回答文本缓冲区上的定时 flush，避免重复调度或遗留失效 timer。
  const clearAnswerTextBufferTimer = useCallback((messageId: string) => {
    const buffer = answerTextBuffersRef.current[messageId];
    if (!buffer?.flushTimer) return;

    clearTimeout(buffer.flushTimer);
    buffer.flushTimer = null;
  }, []);

  // 中文注释：按 messageId 获取或创建文本回答缓冲区，用于暂存尚未合并进 store 的流式分片。
  const getOrCreateAnswerTextBuffer = useCallback(
    (messageId: string): AnswerTextBufferState => {
      const existingBuffer = answerTextBuffersRef.current[messageId];
      if (existingBuffer) return existingBuffer;

      const newBuffer: AnswerTextBufferState = {
        pendingText: '',
        flushTimer: null,
        hasTriggeredStartHaptic: false,
      };
      answerTextBuffersRef.current[messageId] = newBuffer;
      return newBuffer;
    },
    [],
  );

  // 中文注释：将某条回答当前缓冲的文本分片一次性合并到 store；
  // 中间态走低频刷新，结束态/失败态走立即落盘，并在终态后回收缓冲区。
  const flushAnswerTextBuffer = useCallback(
    (
      messageId: string,
      options?: {
        isFinished?: boolean;
        isFailed?: boolean;
        isAborted?: boolean;
      },
    ) => {
      const buffer = answerTextBuffersRef.current[messageId];
      if (!buffer) return;

      // 中文注释：flush 一旦开始就先回收当前 timer，避免消息已被移除时留下失效定时器引用。
      clearAnswerTextBufferTimer(messageId);

      const currentMsg = useMessageStore
        .getState()
        .getMessage(tabId, messageId);
      if (!currentMsg) {
        // 中文注释：消息不在 store 中通常意味着会话已切走、消息被删或组件已进入清理流程，
        // 此时直接丢弃缓冲内容并回收终态引用，避免后续分片继续挂在无效 buffer 上。
        if (options?.isFinished || options?.isFailed || options?.isAborted) {
          delete answerTextBuffersRef.current[messageId];
        }
        return;
      }

      if (
        !buffer.pendingText &&
        !options?.isFinished &&
        !options?.isFailed &&
        !options?.isAborted
      ) {
        return;
      }

      const mergedContent = (currentMsg.answer_content || '') + buffer.pendingText;
      // 中文注释：文本回答终态沿用 isEnd 的收口逻辑，但用户取消要单独落成 canceled，避免和失败态混淆。
      const progress = options?.isFailed
        ? 3
        : options?.isAborted
          ? 4
          : options?.isFinished
            ? 2
            : 1;
      // 中文注释：失败、结束、取消都属于最终态，终态 flush 时要立即持久化到数据库。
      const shouldPersistTerminalState =
        Boolean(options?.isFinished) ||
        Boolean(options?.isFailed) ||
        Boolean(options?.isAborted);
      buffer.pendingText = '';

      updateMessage(
        tabId,
        currentMsg.message_id,
        {
          answer_type: 'text',
          answer_time: Date.now(),
          answer_content: mergedContent,
          progress,
        },
        shouldPersistTerminalState,
      );

      // 中文注释：终态消息写回 store 后立即回收缓冲区，避免旧分片状态残留到下一轮回答。
      if (shouldPersistTerminalState) {
        delete answerTextBuffersRef.current[messageId];
      }
    },
    [clearAnswerTextBufferTimer, tabId, updateMessage],
  );

  const onReceiveMessage = useCallback(
    (data: TActrMessageResponse) => {
      const messageId = data.message_id;
      const currentMsg = useMessageStore
        .getState()
        .getMessage(tabId, messageId);

      if (!currentMsg) {
        console.warn('[ActrInput] No messages in store to update');
        return;
      }

      const isFinished = data.isEnd || data.status === 2;
      const isFailed = data.status === 3;
      // 中文注释：isAbort 表示用户主动取消，本地收口流程与 isEnd 一致，但最终 progress 要保存为 canceled。
      const isAborted = data.isAbort;
      const buffer = getOrCreateAnswerTextBuffer(messageId);

      if (data.message) {
        buffer.pendingText += data.message;
      }

      // 中文注释：首个文本分片只触发一次轻震，避免缓冲窗口内因 store 尚未更新导致重复触发。
      if (!isFailed && !buffer.hasTriggeredStartHaptic) {
        triggerHaptic('impactHeavy');
        buffer.hasTriggeredStartHaptic = true;
      }

      if (isFailed) {
        // 中文注释：失败态要立刻落盘并清理缓冲，避免错误消息还滞留在延时队列里。
        triggerLongVibration(300);
        flushAnswerTextBuffer(messageId, { isFailed: true });
        return;
      }

      // 中文注释：取消态和结束态一样立即收口，不再等待定时器，只是最终 progress 会落成 canceled。
      // 震动已移到 ChatBubble 的 Typewriter onComplete，对齐视觉完成时机。
      if (isAborted) {
        flushAnswerTextBuffer(messageId, { isAborted: true });

        getCurrentFullscreenTab().then((currentTabId) => {
          if (currentTabId === tabId) {
            updateMessage(tabId, currentMsg.message_id, { is_read: 1 }, true);
          } else {
            useUnreadStore.getState().incrementUnread(tabId);
          }
        });
        return;
      }

      // 中文注释：结束分片不再等待定时器，直接合并剩余文本并写入最终态。
      // 震动已移到 ChatBubble 的 Typewriter onComplete，对齐视觉完成时机，
      // 不再在数据层收到 isEnd 时立刻触发（那时打字动画可能还没追完）。
      if (isFinished) {
        flushAnswerTextBuffer(messageId, { isFinished: true });

        getCurrentFullscreenTab().then((currentTabId) => {
          if (currentTabId === tabId) {
            updateMessage(tabId, currentMsg.message_id, { is_read: 1 }, true);
          } else {
            useUnreadStore.getState().incrementUnread(tabId);
          }
        });
        return;
      }

      // 中文注释：流式中间态只在一个短时间窗后集中刷新一次，降低消息洪峰时的 UI/原生桥压力。
      if (!buffer.flushTimer) {
        buffer.flushTimer = setTimeout(() => {
          flushAnswerTextBuffer(messageId);
        }, TEXT_MESSAGE_FLUSH_DELAY_MS);
      }
    },
    [flushAnswerTextBuffer, getOrCreateAnswerTextBuffer, tabId, updateMessage],
  );

  useEffect(() => {
    // 中文注释：hook 卸载时统一清理所有文本缓冲区定时器和缓存，避免切 tab 或组件销毁后继续回写旧分片。
    return () => {
      Object.keys(answerTextBuffersRef.current).forEach((messageId) => {
        clearAnswerTextBufferTimer(messageId);
        delete answerTextBuffersRef.current[messageId];
      });
    };
  }, [clearAnswerTextBufferTimer]);

  const getOrCreateAnswerVoiceBuffer = useCallback(
    (messageId: string): AnswerVoiceBufferState => {
      const existingBuffer = answerVoiceBuffersRef.current[messageId];
      if (existingBuffer) return existingBuffer;

      const newBuffer: AnswerVoiceBufferState = {
        nextSequence: 1,
        pendingBySequence: {},
        orderedChunks: [],
        receivedSequences: [],
      };
      answerVoiceBuffersRef.current[messageId] = newBuffer;
      return newBuffer;
    },
    [],
  );

  const flushAnswerVoiceBuffer = useCallback((messageId: string) => {
    const buffer = answerVoiceBuffersRef.current[messageId];
    if (!buffer) return;

    while (buffer.pendingBySequence[buffer.nextSequence]) {
      buffer.orderedChunks.push(buffer.pendingBySequence[buffer.nextSequence]);
      delete buffer.pendingBySequence[buffer.nextSequence];
      buffer.nextSequence += 1;
    }
  }, []);

  const buildAnswerVoiceChunksForFinalize = useCallback(
    (messageId: string): Uint8Array[] => {
      const buffer = answerVoiceBuffersRef.current[messageId];
      if (!buffer) return [];

      const orderedPendingSequences = Object.keys(buffer.pendingBySequence)
        .map((key) => Number(key))
        .sort((a, b) => a - b);

      return [
        ...buffer.orderedChunks,
        ...orderedPendingSequences.map(
          (sequence) => buffer.pendingBySequence[sequence],
        ),
      ];
    },
    [],
  );

  const finalizeAnswerVoiceMessage = useCallback(
    async (messageId: string) => {
      try {
        const chunks = buildAnswerVoiceChunksForFinalize(messageId);
        if (!chunks?.length) return;

        const voicePayload = await saveVoiceMessageWav({
          messageId: `${messageId}-answer`,
          chunks,
        });
        const serializedVoicePayload = JSON.stringify(voicePayload);
        const currentMsg = useMessageStore
          .getState()
          .getMessage(tabId, messageId);

        // 中文注释：回答语音可能先于问题语音消息入列，先缓存更新，等问题消息补入后再合并。
        if (!currentMsg) {
          pendingVoiceMessageUpdatesRef.current[messageId] = {
            ...(pendingVoiceMessageUpdatesRef.current[messageId] || {}),
            answer_type: 'voice',
            answer_voice_payload: serializedVoicePayload,
            answer_time: Date.now(),
            progress: 2,
          };
          delete answerVoiceBuffersRef.current[messageId];
          return;
        }

        updateMessage(
          tabId,
          messageId,
          {
            answer_type: 'voice',
            answer_voice_payload: serializedVoicePayload,
            answer_time: Date.now(),
            progress: 2,
          },
          true,
        );
        delete answerVoiceBuffersRef.current[messageId];
      } catch (error) {
        console.error('[ActrInput] 保存 answer 语音消息失败:', error);
      }
    },
    [buildAnswerVoiceChunksForFinalize, tabId, updateMessage],
  );

  const handleAnswerVoiceFinishedReadState = useCallback(
    (messageId: string) => {
      getCurrentFullscreenTab().then((currentTabId) => {
        if (currentTabId === tabId) {
          const currentMsg = useMessageStore
            .getState()
            .getMessage(tabId, messageId);
          if (currentMsg) {
            updateMessage(tabId, messageId, { is_read: 1 }, true);
            return;
          }

          pendingVoiceMessageUpdatesRef.current[messageId] = {
            ...(pendingVoiceMessageUpdatesRef.current[messageId] || {}),
            is_read: 1,
          };
        } else {
          useUnreadStore.getState().incrementUnread(tabId);
        }
      });
    },
    [tabId, updateMessage],
  );

  const onReceiveVoice = useCallback(
    (data: TActrMessageResponse) => {
      const messageId = data.message_id;
      const currentMsg = useMessageStore
        .getState()
        .getMessage(tabId, messageId);
      const isFinished = data.isEnd || data.status === 2;
      const fallbackMessage = t('actr.voice.voiceMessageReceived');
      const newContent = currentMsg?.answer_content
        ? currentMsg.answer_content
        : fallbackMessage;

      if (data.message instanceof Uint8Array) {
        const sequence = data.sequence;

        if (typeof sequence === 'number') {
          console.warn(
            `[ActrInput] 收到 answer 语音包: messageId=${messageId}, sequence=${sequence}`,
          );
          const buffer = getOrCreateAnswerVoiceBuffer(messageId);

          if (
            sequence < buffer.nextSequence ||
            buffer.pendingBySequence[sequence]
          ) {
            console.warn(
              `[ActrInput] 收到重复 answer 语音包，已忽略: messageId=${messageId}, sequence=${sequence}`,
            );
          } else {
            buffer.pendingBySequence[sequence] = data.message;
            buffer.receivedSequences.push(sequence);
            buffer.receivedSequences.sort((a, b) => a - b);
            if (isFinished) {
              buffer.endSequence = sequence;
            }
            flushAnswerVoiceBuffer(messageId);
          }

          if (isFinished && buffer.endSequence != null) {
            const hasMissingPackets = buffer.nextSequence <= buffer.endSequence;
            if (hasMissingPackets) {
              const missingSequences: number[] = [];
              for (
                let seq = buffer.nextSequence;
                seq <= buffer.endSequence;
                seq += 1
              ) {
                if (!buffer.pendingBySequence[seq]) {
                  missingSequences.push(seq);
                }
              }
              console.error(
                `[ActrInput] answer 语音包乱序且缺包，仍继续落盘: messageId=${messageId}, endSequence=${buffer.endSequence}, nextSequence=${buffer.nextSequence}, received=${buffer.receivedSequences.join(',')}, missing=${missingSequences.join(',')}`,
              );
            }
          }
        } else {
          console.warn(
            `[ActrInput] answer 语音包缺少 sequence，降级按到达顺序处理: messageId=${messageId}`,
          );
          const buffer = getOrCreateAnswerVoiceBuffer(messageId);
          buffer.orderedChunks.push(data.message);
        }
      }

      const updates: Partial<IMessage> = {
        answer_type: 'voice',
        answer_time: Date.now(),
        answer_content: newContent,
        progress: isFinished ? 2 : 1,
      };

      if (!currentMsg) {
        pendingVoiceMessageUpdatesRef.current[messageId] = {
          ...(pendingVoiceMessageUpdatesRef.current[messageId] || {}),
          ...updates,
        };
        if (isFinished) {
          void finalizeAnswerVoiceMessage(messageId);
          handleAnswerVoiceFinishedReadState(messageId);
        }
        return;
      }

      updateMessage(tabId, messageId, updates, isFinished);
      if (isFinished) {
        void finalizeAnswerVoiceMessage(messageId);
        handleAnswerVoiceFinishedReadState(messageId);
      }
    },
    [
      finalizeAnswerVoiceMessage,
      flushAnswerVoiceBuffer,
      getOrCreateAnswerVoiceBuffer,
      handleAnswerVoiceFinishedReadState,
      t,
      tabId,
      updateMessage,
    ],
  );

  const {
    sendMessage,
    sendVoiceChunk,
    startVoice,
    endVoice,
    session_id,
    connection,
  } = useActrConnection({
    appId,
    tabId,
    config: appInfo
      ? {
          app_id: appInfo.app_id,
          key_id: appInfo.key_id,
          authority_services: appInfo.authority_services,
          signaling_services: appInfo.signaling_services,
          stun_services: appInfo.stun_services,
          turn_services: appInfo.turn_services,
          realm_id: appInfo.realm_id,
          server_actor_type: appInfo.server_actor_type,
        }
      : undefined,
    callbacks: {
      onMessage: onReceiveMessage,
      onVoice: onReceiveVoice,
    },
  });

  const processMessageSend = useCallback(
    async (content: string, options?: { isVoice?: boolean }) => {
      const { isVoice } = options || {};
      // 发送前同步读取网络快照，iOS 上 NetInfo.refresh() 是 no-op，无需 await
      const networkStatus = getNetworkStatusForSend();

      if (networkStatus.shouldBlockSend) {
        showTopToast?.(t('common.networkCheckInfo'));
        return;
      }

      if (appInfo.app_status !== 'alive') {
        GlobalAlert.alert(t('actr.app.notExist'));
        return;
      }

      // if (connection?.reason) {
      //   GlobalAlert.alert(t('actr.auth.serviceInitFailed'), connection.reason);
      //   return;
      // }

      // 实时读取最新的附件和编辑状态，避免闭包陷阱
      const currentInputState = useInputStore.getState().inputs[tabId];
      const attachmentsSnapshot = currentInputState?.attachments
        ? [...currentInputState.attachments]
        : [];
      const editingMessageIdSnapshot =
        currentInputState?.editingMessageId || null;

      const trimmedContent = content.trim();
      // 有附件时允许空文本发送
      if (!trimmedContent && !isVoice && attachmentsSnapshot.length === 0) {
        return;
      }

      if (editingMessageIdSnapshot) {
        const messageIdToDelete = editingMessageIdSnapshot;
        setEditingMessageId(tabId, null);
        try {
          deleteMessageFromStore(tabId, messageIdToDelete);
        } catch (e) {
          console.error(
            '[ActrInput] Failed to delete message during re-edit:',
            e,
          );
        }
      }

      let location: TLocation | undefined;
      if (
        attachmentsSnapshot.length &&
        attachmentsSnapshot[0].attachment_type === 'location'
      ) {
        const local = attachmentsSnapshot[0];
        location = {
          latitude: local.latitude!,
          longitude: local.longitude!,
          address: '',
          placeName: '',
        };
      }

      const newMessage: IMessage = {
        message_id: Utils.generateId(),
        app_id: appId,
        tab_id: tabId,
        session_id: session_id,
        question_type: isVoice ? 'voice' : 'text',
        is_preset: 0,
        question_time: Date.now(),
        question_content: isVoice ? '' : trimmedContent,
        answer_time: 0,
        answer_content: '',
        attachments:
          attachmentsSnapshot.length > 0
            ? JSON.stringify(attachmentsSnapshot)
            : undefined,
        is_edited: editingMessageIdSnapshot ? 1 : 0,
        progress: 0,
      };

      const reqMsg = {
        message_id: newMessage.message_id,
        session_id: newMessage.session_id,
        message: trimmedContent,
        attachments: attachmentsSnapshot.filter(
          (att) => att.attachment_type !== 'location',
        ),
        location,
        localMessage: newMessage,
      };

      // 立即清空输入框，确保在任何异步操作前完成
      clearInputInStore(tabId);

      if (!isVoice) {
        addMessage(tabId, newMessage);
        await sendMessage(reqMsg);
      }

      return reqMsg;
    },
    [
      addMessage,
      appId,
      clearInputInStore,
      connection?.reason,
      deleteMessageFromStore,
      sendMessage,
      session_id,
      setEditingMessageId,
      showTopToast,
      t,
      tabId,
    ],
  );

  const handleSend = useCallback(() => {
    // 实时读取最新的 inputText，避免闭包陷阱
    const currentInputText = useInputStore.getState().inputs[tabId]?.text || '';
    void processMessageSend(currentInputText);
  }, [tabId, processMessageSend]);

  const handleLoginModalClose = useCallback(() => {
    setShowLoginModal(false);
    setAuthParams(null);
  }, []);

  // 登录成功后仅关闭弹窗，不再自动发送（授权已前置到输入框聚焦时）
  const handleLoginSuccess = useCallback(() => {
    setShowLoginModal(false);
    setAuthParams(null);
  }, []);

  const handleAuthorizeClose = useCallback(() => {
    setShowAuthorizeModal(false);
  }, []);

  // 授权成功后仅关闭弹窗，不再自动发送
  const handleAuthorizeSuccess = useCallback(() => {
    setShowAuthorizeModal(false);
  }, []);

  const voiceMessageCallbacks = {
    onVoiceMessageReady: ({
      messageId,
      message,
      voicePayload,
    }: {
      messageId: string;
      message: IMessage;
      voicePayload: string;
    }) => {
      const pendingUpdates = pendingVoiceMessageUpdatesRef.current[messageId];
      const newMessage: IMessage = {
        ...message,
        question_voice_payload: voicePayload,
        progress: pendingUpdates?.progress ?? 1,
        ...pendingUpdates,
      };
      delete pendingVoiceMessageUpdatesRef.current[messageId];
      addMessage(tabId, newMessage);
    },
    onVoiceMessageCanceled: (messageId: string) => {
      delete pendingVoiceMessageUpdatesRef.current[messageId];
      delete answerVoiceBuffersRef.current[messageId];
      const currentMessage = useMessageStore
        .getState()
        .getMessage(tabId, messageId);
      if (currentMessage?.question_voice_payload) {
        void cleanupVoiceMessageFile(currentMessage.question_voice_payload);
      }
      deleteMessageFromStore(tabId, messageId);
    },
  };

  return {
    showLoginModal,
    authParams,
    showAuthorizeModal,
    checkInputAuth,
    processMessageSend,
    handleSend,
    handleLoginModalClose,
    handleLoginSuccess,
    handleAuthorizeClose,
    handleAuthorizeSuccess,
    sendVoiceChunk,
    startVoice,
    endVoice,
    voiceMessageCallbacks,
  };
}
