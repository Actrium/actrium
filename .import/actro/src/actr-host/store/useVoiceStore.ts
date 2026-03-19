import { create } from 'zustand';

export type VoicePlaybackPart = 'question' | 'answer';
export type VoicePlaybackMode = 'voice_message' | 'read_aloud';
export type VoicePlaybackStatus = 'idle' | 'loading' | 'playing';

export interface IVoicePlaybackState {
  appId: string | null;
  messageId: string | null;
  part: VoicePlaybackPart | null;
  mode: VoicePlaybackMode | null;
  status: VoicePlaybackStatus;
}

interface VoiceState {
  activePlayback: IVoicePlaybackState;
  setActivePlayback: (
    payload: Omit<IVoicePlaybackState, 'status'> & {
      status?: Exclude<VoicePlaybackStatus, 'idle'>;
    },
  ) => void;
  updatePlaybackStatus: (
    status: Exclude<VoicePlaybackStatus, 'idle'>,
  ) => void;
  clearPlayback: () => void;
}

const IDLE_PLAYBACK: IVoicePlaybackState = {
  appId: null,
  messageId: null,
  part: null,
  mode: null,
  status: 'idle',
};

/**
 * 全局语音播放状态管理
 * 中文注释：统一维护“哪条消息的哪一部分在以什么模式播放，以及当前阶段”，
 * 避免 question / answer 共用一个 messageId 时出现播放态串联。
 */
export const useVoiceStore = create<VoiceState>((set) => ({
  activePlayback: IDLE_PLAYBACK,
  setActivePlayback: (payload) =>
    set({
      activePlayback: {
        ...payload,
        status: payload.status ?? 'playing',
      },
    }),
  updatePlaybackStatus: (status) =>
    set((state) => ({
      activePlayback:
        state.activePlayback.status === 'idle'
          ? state.activePlayback
          : {
              ...state.activePlayback,
              status,
            },
    })),
  clearPlayback: () => set({ activePlayback: IDLE_PLAYBACK }),
}));
