import { create } from 'zustand';
import { IAttachmentItem } from '../types/attachment';

const EMPTY_ATTACHMENTS: IAttachmentItem[] = [];
const EMPTY_INPUT = { text: '', attachments: EMPTY_ATTACHMENTS, editingMessageId: null as string | null };

interface InputState {
  // key is tabId
  inputs: Record<string, {
    text: string; // 输入文本
    attachments: IAttachmentItem[]; // 附件
    editingMessageId: string | null; // 编辑的消息ID
  }>;
  setInputText: (tabId: string, text: string) => void;
  setAttachments: (tabId: string, attachments: IAttachmentItem[]) => void;
  setEditingMessageId: (tabId: string, messageId: string | null) => void;
  updateAttachment: (tabId: string, attachmentId: string, updates: Partial<IAttachmentItem>) => void;
  clearInput: (tabId: string) => void;
}

export const useInputStore = create<InputState>((set) => ({
  inputs: {},
  setInputText: (tabId, text) =>
    set((state) => ({
      inputs: {
        ...state.inputs,
        [tabId]: {
          ...(state.inputs[tabId] || EMPTY_INPUT),
          text,
        },
      },
    })),
  setAttachments: (tabId, attachments) =>
    set((state) => ({
      inputs: {
        ...state.inputs,
        [tabId]: {
          ...(state.inputs[tabId] || EMPTY_INPUT),
          attachments: attachments || EMPTY_ATTACHMENTS,
        },
      },
    })),
  setEditingMessageId: (tabId, messageId) =>
    set((state) => ({
      inputs: {
        ...state.inputs,
        [tabId]: {
          ...(state.inputs[tabId] || EMPTY_INPUT),
          editingMessageId: messageId,
        },
      },
    })),
  updateAttachment: (tabId, attachmentId, updates) =>
    set((state) => {
      const current = state.inputs[tabId] || EMPTY_INPUT;
      const nextAttachments = current.attachments.map((item) =>
        item.id === attachmentId ? { ...item, ...updates } : item,
      );
      return {
        inputs: {
          ...state.inputs,
          [tabId]: { ...current, attachments: nextAttachments },
        },
      };
    }),
  clearInput: (tabId) =>
    set((state) => ({
      inputs: {
        ...state.inputs,
        [tabId]: EMPTY_INPUT,
      },
    })),
}));

export { EMPTY_ATTACHMENTS };
