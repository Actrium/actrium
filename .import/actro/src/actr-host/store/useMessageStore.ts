import { create } from 'zustand';
import { IMessage } from '../../repository/MessageRepository';
import { MessageRepository } from '../../repository/MessageRepository';

type MessagePart = 'attachments' | 'question' | 'answer';

interface MessageState {
  // Key is appId
  messageIds: Record<string, string[]>;
  messageById: Record<string, Record<string, IMessage>>;
  getMessage: (appId: string, messageId: string) => IMessage | undefined;
  setMessages: (appId: string, msgs: IMessage[]) => void;
  // 新消息加到开头（inverted 列表最下方）
  addMessage: (appId: string, msg: IMessage) => void;
  // 加载更多（旧消息）加到末尾（inverted 列表最上方）
  appendOldMessages: (appId: string, msgs: IMessage[]) => void;
  updateMessage: (
    appId: string,
    messageId: string,
    updates: Partial<IMessage>,
    persist?: boolean,
  ) => void;
  deleteMessage: (appId: string, messageId: string) => void;
  cleanMessage: (appId: string) => void;
  // 选择模式相关 删除时使用
  selectionMode: boolean;
  selectedItems: Record<string, Set<MessagePart>>;
  enterSelectionMode: (
    messageId: string,
    appId?: string,
    defaultPart?: MessagePart,
  ) => void;
  toggleSelection: (messageId: string, part: MessagePart) => void;
  exitSelectionMode: () => void;
  deleteSelectedMessages: (appId: string) => Promise<void>;
}

const buildMessageIndex = (msgs: IMessage[]) => {
  const ids: string[] = [];
  const byId: Record<string, IMessage> = {};

  for (const msg of msgs) {
    if (!msg?.message_id) continue;
    const id = msg.message_id;
    if (!byId[id]) {
      ids.push(id);
    }
    byId[id] = msg;
  }

  return { ids, byId };
};

export const useMessageStore = create<MessageState>((set, get) => ({
  messageIds: {},
  messageById: {},
  selectionMode: false,
  selectedItems: {},
  getMessage: (appId, messageId) => get().messageById[appId]?.[messageId],
  setMessages: (appId, msgs) => {
    const { ids, byId } = buildMessageIndex(msgs);
    set((state) => ({
      messageIds: { ...state.messageIds, [appId]: ids },
      messageById: { ...state.messageById, [appId]: byId },
    }));
  },
  addMessage: (appId, msg) => {
    set((state) => {
      const currentIds = state.messageIds[appId] || [];
      const currentById = state.messageById[appId] || {};
      const exists = !!currentById[msg.message_id];

      return {
        messageIds: {
          ...state.messageIds,
          [appId]: exists ? currentIds : [msg.message_id, ...currentIds],
        },
        messageById: {
          ...state.messageById,
          [appId]: {
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
  appendOldMessages: (appId, msgs) =>
    set((state) => {
      const currentIds = state.messageIds[appId] || [];
      const currentById = state.messageById[appId] || {};
      const appendIds: string[] = [];
      const nextById = { ...currentById };

      for (const msg of msgs) {
        if (!msg?.message_id) continue;
        const id = msg.message_id;
        if (!nextById[id]) {
          appendIds.push(id);
        }
        nextById[id] = msg;
      }

      return {
        messageIds: {
          ...state.messageIds,
          [appId]: [...currentIds, ...appendIds],
        },
        messageById: {
          ...state.messageById,
          [appId]: nextById,
        },
      };
    }),
  updateMessage: (appId, messageId, updates, persist = false) => {
    let persistedMsg: IMessage | undefined;

    set((state) => {
      const currentById = state.messageById[appId] || {};
      const currentMsg = currentById[messageId];
      if (!currentMsg) return state;

      const updatedMsg = { ...currentMsg, ...updates };
      persistedMsg = updatedMsg;

      return {
        messageById: {
          ...state.messageById,
          [appId]: {
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
  deleteMessage: (appId, messageId) => {
    set((state) => {
      const currentIds = state.messageIds[appId] || [];
      const currentById = state.messageById[appId] || {};
      if (!currentById[messageId]) return state;

      const nextById = { ...currentById };
      delete nextById[messageId];

      return {
        messageIds: {
          ...state.messageIds,
          [appId]: currentIds.filter((id) => id !== messageId),
        },
        messageById: {
          ...state.messageById,
          [appId]: nextById,
        },
      };
    });

    // 异步持久化
    MessageRepository.deleteMessage(messageId).catch((e) =>
      console.error('[useMessageStore] Failed to delete message from DB:', e),
    );
  },
  cleanMessage: (appId) =>
    set((state) => {
      const newMessageIds = { ...state.messageIds };
      const newMessageById = { ...state.messageById };
      delete newMessageIds[appId];
      delete newMessageById[appId];
      return { messageIds: newMessageIds, messageById: newMessageById };
    }),
  enterSelectionMode: (
    messageId: string,
    appId?: string,
    defaultPart?: MessagePart,
  ) => {
    const state = get();

    // 如果没有提供 appId，尝试从所有消息中查找
    let message: IMessage | undefined;
    if (appId) {
      message = state.messageById[appId]?.[messageId];
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
      } else if (defaultPart === 'question' && message.question_content) {
        selectedParts.add('question');
      } else if (defaultPart === 'answer' && message.answer_content) {
        selectedParts.add('answer');
      }
    } else {
      if (message.attachments) selectedParts.add('attachments');
      if (message.question_content) selectedParts.add('question');
      if (message.answer_content) selectedParts.add('answer');
    }

    set({
      selectionMode: true,
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
      selectionMode: false,
      selectedItems: {},
    });
  },
  deleteSelectedMessages: async (appId: string) => {
    const state = get();
    const selectedItems = state.selectedItems;
    const byId = state.messageById[appId] || {};

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
      if (message.question_content) allParts.add('question');
      if (message.answer_content) allParts.add('answer');

      if (parts.size === allParts.size && parts.size > 0) {
        messagesToDelete.push(messageId);
      } else {
        const updates: Partial<IMessage> = {};
        if (parts.has('attachments')) {
          updates.attachments = undefined;
        }
        if (parts.has('question')) {
          updates.question_content = '';
        }
        if (parts.has('answer')) {
          updates.answer_content = '';
        }
        if (Object.keys(updates).length > 0) {
          messagesToUpdate.push({ messageId, updates });
        }
      }
    }

    // 批量更新 store（一次性更新，减少重新渲染）
    set((innerState) => {
      const currentIds = innerState.messageIds[appId] || [];
      const currentById = innerState.messageById[appId] || {};

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
          [appId]: currentIds.filter((id) => !messagesToDelete.includes(id)),
        },
        messageById: {
          ...innerState.messageById,
          [appId]: nextById,
        },
        selectionMode: false,
        selectedItems: {},
      };
    });

    // 数据库操作在后台异步执行，不阻塞 UI
    Promise.all([
      ...messagesToDelete.map(async (messageId) => {
        try {
          await MessageRepository.deleteMessage(messageId);
        } catch (error) {
          console.error('[useMessageStore] Failed to delete message from DB:', error);
        }
      }),
      ...messagesToUpdate.map(async ({ messageId, updates }) => {
        try {
          const message = byId[messageId];
          if (message) {
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
