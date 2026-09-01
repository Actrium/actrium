import { create } from 'zustand';
import { IMessage } from '../../repository/MessageRepository';
import { MessageRepository } from '../../repository/MessageRepository';
import { cleanupVoiceMessageFile } from '../utils/VoiceMessageStorage';

export type MessagePart = 'attachments' | 'question' | 'answer';

// Select mode type,'none' No selection mode entered,'delete' To remove multiples,'share' To share multiple choices.
export type SelectionMode = 'none' | 'delete' | 'share';

interface MessageState {
  // Key is tabId
  messageIds: Record<string, string[]>;
  messageById: Record<string, Record<string, IMessage>>;
  getMessage: (tabId: string, messageId: string) => IMessage | undefined;
  setMessages: (tabId: string, msgs: IMessage[]) => void;
  // Positive Sequence Table: Add New Message to End
  addMessage: (tabId: string, msg: IMessage) => void;
  // Regular Table: Add to the beginning when loading more old messages
  appendOldMessages: (tabId: string, msgs: IMessage[]) => void;
  updateMessage: (
    tabId: string,
    messageId: string,
    updates: Partial<IMessage>,
    persist?: boolean,
  ) => void;
  deleteMessage: (tabId: string, messageId: string) => void;
  cleanMessage: (tabId: string) => void;
  // Select the mode to support deletion and sharing of both uses.
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

  // When messages are deleted or partially deleted, you can always recollect questions here./Answer the corresponding local voice files and avoid missing from the different entrances.
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

    // Longer and more.
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

    // Longer and more.
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

    // If not provided tabId, try to find from all messages
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

    // Select only the default part if it is specified; otherwise all parts are selected
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

    // Collecting messages to be deleted and updates
    const messagesToDelete: string[] = [];
    const messagesToUpdate: Array<{
      messageId: string;
      updates: Partial<IMessage>;
    }> = [];

    for (const [messageId, parts] of Object.entries(selectedItems)) {
      const message = byId[messageId];
      if (!message) continue;

      // Check if all parts are selected
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

    // Batch Update store(one-time update, less re-rendering)
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

    // Database operations are performed in the backstage, without blocking UI
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
