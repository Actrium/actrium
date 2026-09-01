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

// Text flow fractions merge in this time window and write again store, avoid every one chunk Other Organiser
const TEXT_MESSAGE_FLUSH_DELAY_MS = 48;

interface UseActrMessageFlowResult {
  showLoginModal: boolean;
  authParams: {
    clientId: string;
    appId: string;
  } | null;
  showAuthorizeModal: boolean;
  /** Enter box to call before focus, pop up login when not authorized/Permission to bomb windows and return. false */
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
  // Maintenance of an independent buffer zone and timer for each text response, and consolidation of HF current fractions into low frequency UI Refresh.
  const answerTextBuffersRef = useRef<Record<string, AnswerTextBufferState>>(
    {},
  );

  const handleAuthStatus = useCallback(
    (status: AuthStatus, clientId: string): boolean => {
      if (status === 'authorized') {
        return true;
      }

      if (status === 'not_logged_in') {
        // Pad and Phone Login window with current page, no longer jump login tab.
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
   * Enter the pre-check that you want to pre-screen before you focus on the box.
   * Pop-up login when not authorized/Permission to bomb windows and return. false, is authorized to return true.
   */
  const checkInputAuth = useCallback(async (): Promise<boolean> => {
    const { status, clientId } = await checkAuth();
    return handleAuthStatus(status, clientId);
  }, [checkAuth, handleAuthStatus]);

  // Clear the time on the specified text response buffer flush, avoid duplication of movement or legacy failures timer.
  const clearAnswerTextBufferTimer = useCallback((messageId: string) => {
    const buffer = answerTextBuffersRef.current[messageId];
    if (!buffer?.flushTimer) return;

    clearTimeout(buffer.flushTimer);
    buffer.flushTimer = null;
  }, []);

  // Press messageId Get or create text to answer the buffer for temporary storage not yet merged store - The flow of the fraction.
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

  // Merges a piece of text that answers the current buffer to a single time store;
  // Middle-step low-frequency refreshing, end-state/The failure to move immediately sets the wheel and then recycles the buffer zone after it is final.
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

      // flush Recycle the current one as soon as it starts. timer, to avoid leaving a invalid timer reference when the message has been removed.
      clearAnswerTextBufferTimer(messageId);

      const currentMsg = useMessageStore
        .getState()
        .getMessage(tabId, messageId);
      if (!currentMsg) {
        // The message is not here. store The first of these is the fact that the session has been removed, the message deleted or the components are in the clean-up process.
        // Drop the buffer and recover the final reference at this time to avoid the subsequent fraction remaining on the unworkable buffer Go on.
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
      // Text answers remain the same isEnd ..the logic of the end-of-door, but the user cancels to be created on its own. canceledAnd avoid confusion with failure.
      const progress = options?.isFailed
        ? 3
        : options?.isAborted
          ? 4
          : options?.isFinished
            ? 2
            : 1;
      // Failure, termination, cancellation are final. flush , which is then immediately and permanently integrated into the database.
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

      // Final Message Return store The buffer zone is then recovered and the old fraction remains in the next round of responses.
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
      // isAbort The user is taking the initiative to cancel the local access process and isEnd It's the same, but eventually... progress To Save As canceled.
      const isAborted = data.isAbort;
      const buffer = getOrCreateAnswerTextBuffer(messageId);

      if (data.message) {
        buffer.pendingText += data.message;
      }

      // The first text fraction triggers only one light shock, avoiding the internal causes of the buffer window store Not updated to trigger repetition.
      if (!isFailed && !buffer.hasTriggeredStartHaptic) {
        triggerHaptic('impactHeavy');
        buffer.hasTriggeredStartHaptic = true;
      }

      if (isFailed) {
        // The failure must be immediately set and cleared to avoid the false information remaining in the delay queue.
        triggerLongVibration(300);
        flushAnswerTextBuffer(messageId, { isFailed: true });
        return;
      }

      // The cancellation is as immediate as the end, no longer waiting for the timer, but only finally progress It's gonna happen. canceled.
      // The vibration has moved to ChatBubble It's... Typewriter onCompleteAnd match the visual time.
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

      // Ends the segment without waiting for the timer, by combining the remaining text directly and writing to the final state.
      // The vibration has moved to ChatBubble It's... Typewriter onComplete# And match the time of the vision #
      // No more data receptions on the data level isEnd It's a real thing, and it's a real thing.
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

      // The current meso only concentrates on a single window, lowers the news peak. UI/Original bridge pressure.
      if (!buffer.flushTimer) {
        buffer.flushTimer = setTimeout(() => {
          flushAnswerTextBuffer(messageId);
        }, TEXT_MESSAGE_FLUSH_DELAY_MS);
      }
    },
    [flushAnswerTextBuffer, getOrCreateAnswerTextBuffer, tabId, updateMessage],
  );

  useEffect(() => {
    // hook Unify all text buffer timers and caches on unmounting, avoid cutting tab or the component is destroyed and continues to write back old pieces.
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

        // Answering a voice may be before entering the question voice message, pre-empting updates, adding questions and so on before merging.
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
        console.error('[ActrInput] Save answer Voice message failed:', error);
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
            `[ActrInput] Copy that. answer Speech pack: messageId=${messageId}, sequence=${sequence}`,
          );
          const buffer = getOrCreateAnswerVoiceBuffer(messageId);

          if (
            sequence < buffer.nextSequence ||
            buffer.pendingBySequence[sequence]
          ) {
            console.warn(
              `[ActrInput] Copy that. answer Speech pack, ignored: messageId=${messageId}, sequence=${sequence}`,
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
                `[ActrInput] answer The voice is out of order and missing, and the wheel continues.: messageId=${messageId}, endSequence=${buffer.endSequence}, nextSequence=${buffer.nextSequence}, received=${buffer.receivedSequences.join(',')}, missing=${missingSequences.join(',')}`,
              );
            }
          }
        } else {
          console.warn(
            `[ActrInput] answer Speech pack missing sequence, downgraded by arrival order: messageId=${messageId}`,
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
      // The video is a good one for the Internet and is a good one for the Internet.iOS Go, go, go! NetInfo.refresh() Yes. no-opNo, I don't. await
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

      // Read up-to-date attachments and editing status in real time, avoiding the closed trap
      const currentInputState = useInputStore.getState().inputs[tabId];
      const attachmentsSnapshot = currentInputState?.attachments
        ? [...currentInputState.attachments]
        : [];
      const editingMessageIdSnapshot =
        currentInputState?.editingMessageId || null;

      const trimmedContent = content.trim();
      // Allow empty text to send if attachments are available
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

      // Empty input boxes immediately to ensure that any stept operations are completed
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
    // Read the latest in real time inputTextAvoid the trap.
    const currentInputText = useInputStore.getState().inputs[tabId]?.text || '';
    void processMessageSend(currentInputText);
  }, [tabId, processMessageSend]);

  const handleLoginModalClose = useCallback(() => {
    setShowLoginModal(false);
    setAuthParams(null);
  }, []);

  // Only closes the bullet window after login is successful and no automatic sending (authorization pre-enabled to focus input)
  const handleLoginSuccess = useCallback(() => {
    setShowLoginModal(false);
    setAuthParams(null);
  }, []);

  const handleAuthorizeClose = useCallback(() => {
    setShowAuthorizeModal(false);
  }, []);

  // Only bullet windows are closed after the authorization is successful and no more automatic transmissions
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
