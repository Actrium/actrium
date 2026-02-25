import { create } from 'zustand';
import { IMessage } from '../../repository/MessageRepository';
import { MessageRepository } from '../../repository/MessageRepository';

type MessagePart = 'attachments' | 'question' | 'answer';

interface MessageState {
  // Key is appId
  messages: Record<string, IMessage[]>;
  setMessages: (appId: string, msgs: IMessage[]) => void;
  // 新消息加到开头（inverted 列表最下方）
  addMessage: (appId: string, msg: IMessage) => void;
  // 加载更多（旧消息）加到末尾（inverted 列表最上方）
  appendOldMessages: (appId: string, msgs: IMessage[]) => void;
  updateMessage: (appId: string, messageId: string, updates: Partial<IMessage>, persist?: boolean) => void;
  deleteMessage: (appId: string, messageId: string) => void;
  cleanMessage: (appId: string) => void;
  // 选择模式相关 删除时使用
  selectionMode: boolean;
  selectedItems: Record<string, Set<MessagePart>>;
  enterSelectionMode: (messageId: string, appId?: string, defaultPart?: MessagePart) => void;
  toggleSelection: (messageId: string, part: MessagePart) => void;
  exitSelectionMode: () => void;
  deleteSelectedMessages: (appId: string) => Promise<void>;
}

export const useMessageStore = create<MessageState>((set, get) => ({
  messages: {},
  selectionMode: false,
  selectedItems: {},
  setMessages: (appId, msgs) =>
    set((state) => ({
      messages: { ...state.messages, [appId]: msgs },
    })),
  addMessage: (appId, msg) => {
    set((state) => ({
      messages: {
        ...state.messages,
        [appId]: [msg, ...(state.messages[appId] || [])],
      },
    }));
    // 异步持久化
    MessageRepository.saveMessage(msg).catch((e) =>
      console.error('[useMessageStore] Failed to save added message:', e),
    );
  },
  appendOldMessages: (appId, msgs) =>
    set((state) => ({
      messages: {
        ...state.messages,
        [appId]: [...(state.messages[appId] || []), ...msgs],
      },
    })),
  updateMessage: (appId, messageId, updates, persist = false) => {
    set((state) => {
      const currentMessages = state.messages[appId] || [];
      const updatedMessages = currentMessages.map((m) =>
        m.message_id === messageId ? { ...m, ...updates } : m,
      );

      // 如果需要持久化，找到更新后的完整对象
      if (persist) {
        const updatedMsg = updatedMessages.find((m) => m.message_id === messageId);
        if (updatedMsg) {
          MessageRepository.saveMessage(updatedMsg).catch((e) =>
            console.error('[useMessageStore] Failed to update message in DB:', e),
          );
        }
      }

      return {
        messages: {
          ...state.messages,
          [appId]: updatedMessages,
        },
      };
    });
  },
  deleteMessage: (appId, messageId) => {
    set((state) => ({
      messages: {
        ...state.messages,
        [appId]: (state.messages[appId] || []).filter(
          (m) => m.message_id !== messageId,
        ),
      },
    }));
    // 异步持久化
    MessageRepository.deleteMessage(messageId).catch((e) =>
      console.error('[useMessageStore] Failed to delete message from DB:', e),
    );
  },
  cleanMessage: (appId) =>
    set((state) => {
      const newMessages = { ...state.messages };
      delete newMessages[appId];
      return { messages: newMessages };
    }),
  enterSelectionMode: (messageId: string, appId?: string, defaultPart?: MessagePart) => {
    const state = get();
    
    // 如果没有提供 appId，尝试从所有消息中查找
    let message: IMessage | undefined;
    if (appId) {
      message = state.messages[appId]?.find((m) => m.message_id === messageId);
    } else {
      // 遍历所有 appId 查找消息
      for (const msgs of Object.values(state.messages)) {
        message = msgs.find((m) => m.message_id === messageId);
        if (message) break;
      }
    }
    
    if (!message) return;

    const selectedParts = new Set<MessagePart>();
    
    // 如果指定了默认部分，只选中该部分；否则选中所有部分
    if (defaultPart) {
      // 验证该部分是否存在
      if (defaultPart === 'attachments' && message.attachments) {
        selectedParts.add('attachments');
      } else if (defaultPart === 'question' && message.question_content) {
        selectedParts.add('question');
      } else if (defaultPart === 'answer' && message.answer_content) {
        selectedParts.add('answer');
      }
    } else {
      // 默认选中所有部分
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
    const messages = state.messages[appId] || [];

    // 收集需要删除的消息和需要更新的消息
    const messagesToDelete: string[] = [];
    const messagesToUpdate: Array<{ messageId: string; updates: Partial<IMessage> }> = [];

    for (const [messageId, parts] of Object.entries(selectedItems)) {
      const message = messages.find((m) => m.message_id === messageId);
      if (!message) continue;

      // 检查是否所有部分都被选中
      const allParts = new Set<MessagePart>();
      if (message.attachments) allParts.add('attachments');
      if (message.question_content) allParts.add('question');
      if (message.answer_content) allParts.add('answer');

      if (parts.size === allParts.size && parts.size > 0) {
        // 删除整条消息
        messagesToDelete.push(messageId);
      } else {
        // 部分删除，重置对应字段
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
    set((state) => {
      const currentMessages = state.messages[appId] || [];
      
      // 删除整条消息
      const filteredMessages = currentMessages.filter(
        (m) => !messagesToDelete.includes(m.message_id)
      );
      
      // 更新部分删除的消息
      const updatedMessages = filteredMessages.map((m) => {
        const updateItem = messagesToUpdate.find((item) => item.messageId === m.message_id);
        if (updateItem) {
          return { ...m, ...updateItem.updates };
        }
        return m;
      });

      return {
        messages: {
          ...state.messages,
          [appId]: updatedMessages,
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
          const message = messages.find((m) => m.message_id === messageId);
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
