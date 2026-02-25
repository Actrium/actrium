import { create } from 'zustand';
import { IAttachmentItem } from '../types/attachment';

const EMPTY_ATTACHMENTS: IAttachmentItem[] = [];
const EMPTY_INPUT = { text: '', attachments: EMPTY_ATTACHMENTS, editingMessageId: null as string | null };

interface InputState {
  // key is appId
  inputs: Record<string, {
    text: string; // 输入文本
    attachments: IAttachmentItem[]; // 附件
    editingMessageId: string | null; // 编辑的消息ID
  }>;
  setInputText: (appId: string, text: string) => void;
  setAttachments: (appId: string, attachments: IAttachmentItem[]) => void;
  setEditingMessageId: (appId: string, messageId: string | null) => void;
  updateAttachment: (appId: string, attachmentId: string, updates: Partial<IAttachmentItem>) => void;
  clearInput: (appId: string) => void;
}

export const useInputStore = create<InputState>((set) => ({
  inputs: {},
  setInputText: (appId, text) =>
    set((state) => ({
      inputs: {
        ...state.inputs,
        [appId]: {
          ...(state.inputs[appId] || EMPTY_INPUT),
          text,
        },
      },
    })),
  setAttachments: (appId, attachments) =>
    set((state) => ({
      inputs: {
        ...state.inputs,
        [appId]: {
          ...(state.inputs[appId] || EMPTY_INPUT),
          attachments: attachments || EMPTY_ATTACHMENTS,
        },
      },
    })),
  setEditingMessageId: (appId, messageId) =>
    set((state) => ({
      inputs: {
        ...state.inputs,
        [appId]: {
          ...(state.inputs[appId] || EMPTY_INPUT),
          editingMessageId: messageId,
        },
      },
    })),
  updateAttachment: (appId, attachmentId, updates) =>
    set((state) => {
      const current = state.inputs[appId] || EMPTY_INPUT;
      const nextAttachments = current.attachments.map((item) =>
        item.id === attachmentId ? { ...item, ...updates } : item,
      );
      return {
        inputs: {
          ...state.inputs,
          [appId]: { ...current, attachments: nextAttachments },
        },
      };
    }),
  clearInput: (appId) =>
    set((state) => ({
      inputs: {
        ...state.inputs,
        [appId]: EMPTY_INPUT,
      },
    })),
}));

export { EMPTY_ATTACHMENTS };
