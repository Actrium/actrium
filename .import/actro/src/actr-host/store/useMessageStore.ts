import { create } from 'zustand';
import { IMessage } from '../../repository/MessageRepository';
import { MessageRepository } from '../../repository/MessageRepository';
import { cleanupVoiceMessageFile } from '../utils/VoiceMessageStorage';

export type MessagePart = 'attachments' | 'question' | 'answer';

// 选择模式类型，'none' 未进入选择模式，'delete' 为删除多选，'share' 为分享多选。
export type SelectionMode = 'none' | 'delete' | 'share';

interface MessageState {
  // Key is tabId
  messageIds: Record<string, string[]>;
  messageById: Record<string, Record<string, IMessage>>;
  getMessage: (tabId: string, messageId: string) => IMessage | undefined;
  setMessages: (tabId: string, msgs: IMessage[]) => void;
  // 正序列表：新消息追加到末尾
  addMessage: (tabId: string, msg: IMessage) => void;
  // 正序列表：加载更多旧消息时追加到开头
  appendOldMessages: (tabId: string, msgs: IMessage[]) => void;
  updateMessage: (
    tabId: string,
    messageId: string,
    updates: Partial<IMessage>,
    persist?: boolean,
  ) => void;
  deleteMessage: (tabId: string, messageId: string) => void;
  cleanMessage: (tabId: string) => void;
  // 选择模式，支持删除和分享两种用途。
  selectionMode: SelectionMode;
  selectedItems: Record<string, Set<MessagePart>>;
  enterSelectionMode: (
    messageId: string,
    tabId?: string,
    defaultPart?: MessagePart,
    mode?: 'delete' | 'share',
  ) => void;
  toggleSelection: (messageId: string, part: MessagePart) => void;
  exitSelectionMode: () => void;
  deleteSelectedMessages: (tabId: string) => Promise<void>;
}

const buildMessageIndex = (msgs: IMessage[]) => {
  const sortedMsgs = [...msgs].sort((a, b) => {
    if (a.question_time !== b.question_time) {
      return a.question_time - b.question_time;
    }
    return a.message_id.localeCompare(b.message_id);
  });
  const ids: string[] = [];
  const byId: Record<string, IMessage> = {};

  for (const msg of sortedMsgs) {
    if (!msg?.message_id) continue;
    const id = msg.message_id;
    if (!byId[id]) {
      ids.push(id);
    }
    byId[id] = msg;
  }

  return { ids, byId };
};

const cleanupMessageVoiceFiles = async (
  message?: Pick<IMessage, 'question_voice_payload' | 'answer_voice_payload'>,
) => {
  if (!message) return;

  // 消息删除或局部删除时，统一在这里回收提问/回答对应的本地语音文件，避免不同入口遗漏。
  if (message.question_voice_payload) {
    await cleanupVoiceMessageFile(message.question_voice_payload);
  }
  if (message.answer_voice_payload) {
    await cleanupVoiceMessageFile(message.answer_voice_payload);
  }
};

export const useMessageStore = create<MessageState>((set, get) => ({
  messageIds: {},
  messageById: {},
  selectionMode: 'none',
  selectedItems: {},
  getMessage: (tabId, messageId) => get().messageById[tabId]?.[messageId],
  setMessages: (tabId, msgs) => {
    const { ids, byId } = buildMessageIndex(msgs);
    set((state) => ({
      messageIds: { ...state.messageIds, [tabId]: ids },
      messageById: { ...state.messageById, [tabId]: byId },
    }));
  },
  addMessage: (tabId, msg) => {
    set((state) => {
      const currentIds = state.messageIds[tabId] || [];
      const currentById = state.messageById[tabId] || {};
      const exists = !!currentById[msg.message_id];

      return {
        messageIds: {
          ...state.messageIds,
          [tabId]: exists ? currentIds : [...currentIds, msg.message_id],
        },
        messageById: {
          ...state.messageById,
          [tabId]: {
            ...currentById,
            [msg.message_id]: msg,
          },
        },
      };
    });

    // 异步持久化
    MessageRepository.saveMessage(msg).catch((e) =>
      console.error('[useMessageStore] Failed to save added message:', e),
    );
  },
  appendOldMessages: (tabId, msgs) =>
    set((state) => {
      const currentIds = state.messageIds[tabId] || [];
      const currentById = state.messageById[tabId] || {};
      const prependIds: string[] = [];
      const nextById = { ...currentById };
      const sortedMsgs = [...msgs].sort((a, b) => {
        if (a.question_time !== b.question_time) {
          return a.question_time - b.question_time;
        }
        return a.message_id.localeCompare(b.message_id);
      });

      for (const msg of sortedMsgs) {
        if (!msg?.message_id) continue;
        const id = msg.message_id;
        if (!nextById[id]) {
          prependIds.push(id);
        }
        nextById[id] = msg;
      }

      return {
        messageIds: {
          ...state.messageIds,
          [tabId]: [...prependIds, ...currentIds],
        },
        messageById: {
          ...state.messageById,
          [tabId]: nextById,
        },
      };
    }),
  updateMessage: (tabId, messageId, updates, persist = false) => {
    let persistedMsg: IMessage | undefined;

    set((state) => {
      const currentById = state.messageById[tabId] || {};
      const currentMsg = currentById[messageId];
      if (!currentMsg) return state;

      const updatedMsg = { ...currentMsg, ...updates };
      persistedMsg = updatedMsg;

      return {
        messageById: {
          ...state.messageById,
          [tabId]: {
            ...currentById,
            [messageId]: updatedMsg,
          },
        },
      };
    });

    if (persist && persistedMsg) {
      MessageRepository.saveMessage(persistedMsg).catch((e) =>
        console.error('[useMessageStore] Failed to update message in DB:', e),
      );
    }
  },
  deleteMessage: (tabId, messageId) => {
    let deletedMessage: IMessage | undefined;

    set((state) => {
      const currentIds = state.messageIds[tabId] || [];
      const currentById = state.messageById[tabId] || {};
      if (!currentById[messageId]) return state;
      deletedMessage = currentById[messageId];

      const nextById = { ...currentById };
      delete nextById[messageId];

      return {
        messageIds: {
          ...state.messageIds,
          [tabId]: currentIds.filter((id) => id !== messageId),
        },
        messageById: {
          ...state.messageById,
          [tabId]: nextById,
        },
      };
    });

    void cleanupMessageVoiceFiles(deletedMessage);

    // 异步持久化
    MessageRepository.deleteMessage(messageId).catch((e) =>
      console.error('[useMessageStore] Failed to delete message from DB:', e),
    );
  },
  cleanMessage: (tabId) =>
    set((state) => {
      const newMessageIds = { ...state.messageIds };
      const newMessageById = { ...state.messageById };
      delete newMessageIds[tabId];
      delete newMessageById[tabId];
      return { messageIds: newMessageIds, messageById: newMessageById };
    }),
  enterSelectionMode: (
    messageId: string,
    tabId?: string,
    defaultPart?: MessagePart,
    mode: 'delete' | 'share' = 'delete',
  ) => {
    const state = get();

    // 如果没有提供 tabId，尝试从所有消息中查找
    let message: IMessage | undefined;
    if (tabId) {
      message = state.messageById[tabId]?.[messageId];
    } else {
      for (const byId of Object.values(state.messageById)) {
        message = byId[messageId];
        if (message) break;
      }
    }

    if (!message) return;

    const selectedParts = new Set<MessagePart>();

    // 如果指定了默认部分，只选中该部分；否则选中所有部分
    if (defaultPart) {
      if (defaultPart === 'attachments' && message.attachments) {
        selectedParts.add('attachments');
      } else if (
        defaultPart === 'question' &&
        (message.question_content || message.question_type === 'voice')
      ) {
        selectedParts.add('question');
      } else if (defaultPart === 'answer' && (message.answer_content || message.answer_type === 'voice')) {
        selectedParts.add('answer');
      }
    } else {
      if (message.attachments) selectedParts.add('attachments');
      if (message.question_content || message.question_type === 'voice') {
        selectedParts.add('question');
      }
      if (message.answer_content || message.answer_type === 'voice') selectedParts.add('answer');
    }

    set({
      selectionMode: mode,
      selectedItems: {
        [messageId]: selectedParts,
      },
    });
  },
  toggleSelection: (messageId: string, part: MessagePart) => {
    set((state) => {
      const currentSet = state.selectedItems[messageId] || new Set<MessagePart>();
      const newSet = new Set(currentSet);

      if (newSet.has(part)) {
        newSet.delete(part);
      } else {
        newSet.add(part);
      }

      return {
        selectedItems: {
          ...state.selectedItems,
          [messageId]: newSet,
        },
      };
    });
  },
  exitSelectionMode: () => {
    set({
      selectionMode: 'none',
      selectedItems: {},
    });
  },
  deleteSelectedMessages: async (tabId: string) => {
    const state = get();
    const selectedItems = state.selectedItems;
    const byId = state.messageById[tabId] || {};

    // 收集需要删除的消息和需要更新的消息
    const messagesToDelete: string[] = [];
    const messagesToUpdate: Array<{
      messageId: string;
      updates: Partial<IMessage>;
    }> = [];

    for (const [messageId, parts] of Object.entries(selectedItems)) {
      const message = byId[messageId];
      if (!message) continue;

      // 检查是否所有部分都被选中
      const allParts = new Set<MessagePart>();
      if (message.attachments) allParts.add('attachments');
      if (message.question_content || message.question_type === 'voice') {
        allParts.add('question');
      }
      if (message.answer_content || message.answer_type === 'voice') allParts.add('answer');

      if (parts.size === allParts.size && parts.size > 0) {
        messagesToDelete.push(messageId);
      } else {
        const updates: Partial<IMessage> = {};
        if (parts.has('attachments')) {
          updates.attachments = undefined;
        }
        if (parts.has('question')) {
          updates.question_content = '';
          updates.question_voice_payload = undefined;
        }
        if (parts.has('answer')) {
          updates.answer_content = '';
          updates.answer_voice_payload = undefined;
        }
        if (Object.keys(updates).length > 0) {
          messagesToUpdate.push({ messageId, updates });
        }
      }
    }

    // 批量更新 store（一次性更新，减少重新渲染）
    set((innerState) => {
      const currentIds = innerState.messageIds[tabId] || [];
      const currentById = innerState.messageById[tabId] || {};

      const nextById = { ...currentById };

      for (const messageId of messagesToDelete) {
        delete nextById[messageId];
      }

      for (const { messageId, updates } of messagesToUpdate) {
        const msg = nextById[messageId];
        if (msg) {
          nextById[messageId] = { ...msg, ...updates };
        }
      }

      return {
        messageIds: {
          ...innerState.messageIds,
          [tabId]: currentIds.filter((id) => !messagesToDelete.includes(id)),
        },
        messageById: {
          ...innerState.messageById,
          [tabId]: nextById,
        },
        selectionMode: 'none',
        selectedItems: {},
      };
    });

    // 数据库操作在后台异步执行，不阻塞 UI
    Promise.all([
      ...messagesToDelete.map(async (messageId) => {
        try {
          await cleanupMessageVoiceFiles(byId[messageId]);
          await MessageRepository.deleteMessage(messageId);
        } catch (error) {
          console.error('[useMessageStore] Failed to delete message from DB:', error);
        }
      }),
      ...messagesToUpdate.map(async ({ messageId, updates }) => {
        try {
          const message = byId[messageId];
          if (message) {
            if (updates.question_voice_payload === undefined && message.question_voice_payload) {
              await cleanupVoiceMessageFile(message.question_voice_payload);
            }
            if (updates.answer_voice_payload === undefined && message.answer_voice_payload) {
              await cleanupVoiceMessageFile(message.answer_voice_payload);
            }
            await MessageRepository.saveMessage({ ...message, ...updates });
          }
        } catch (error) {
          console.error('[useMessageStore] Failed to update message in DB:', error);
        }
      }),
    ]).catch((error) => {
      console.error('[useMessageStore] Error in background database operations:', error);
    });
  },
}));
