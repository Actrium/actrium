import { create } from 'zustand';

interface VoiceState {
  playingId: string | null;
  appId: string | null;
  setPlayingId: (appId: string | null, id: string | null) => void;
}

/**
 * 全局朗读状态管理
 * 确保同一时间只有一个气泡在播放
 */
export const useVoiceStore = create<VoiceState>((set) => ({
  playingId: null,
  appId: null,
  setPlayingId: (appId, id) => set({ appId, playingId: id }),
}));
