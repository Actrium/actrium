import { create } from 'zustand';

interface FeedbackFormState {
  visible: boolean;
  tabId: string;
  appId: string;
  messageId: string;
  show: (params: { tabId: string; appId: string; messageId: string }) => void;
  hide: () => void;
}

export const useFeedbackFormStore = create<FeedbackFormState>((set) => ({
  visible: false,
  tabId: '',
  appId: '',
  messageId: '',
  show: (params) => set({ visible: true, ...params }),
  hide: () => set({ visible: false }),
}));
