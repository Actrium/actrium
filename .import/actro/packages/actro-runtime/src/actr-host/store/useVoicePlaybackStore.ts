import { create } from 'zustand';
import type { TTSSpeakOptions } from '../../native-bridge';

export type VoicePlaybackPart = 'question' | 'answer';
export type VoicePlaybackMode = 'voice_message' | 'read_aloud';
export type VoicePlaybackStatus = 'idle' | 'loading' | 'playing';

export interface IVoicePlaybackState {
  appId: string | null;
  messageId: string | null;
  part: VoicePlaybackPart | null;
  mode: VoicePlaybackMode | null;
  status: VoicePlaybackStatus;
  // 朗读模式切换到 pending 后，需要保留目标文本，等原生 finish 事件回来再真正 speak。
  text?: string;
  // 语速、音高等朗读参数要和目标一起缓存，避免 stop -> finish 期间丢失配置。
  ttsOptions?: TTSSpeakOptions;
}

interface VoiceState {
  activePlayback: IVoicePlaybackState;
  pendingPlayback: IVoicePlaybackState | null;
  setActivePlayback: (
    payload: Omit<IVoicePlaybackState, 'status'> & {
      status?: Exclude<VoicePlaybackStatus, 'idle'>;
    },
  ) => void;
  setPendingPlayback: (
    payload: Omit<IVoicePlaybackState, 'status'> & {
      status?: Exclude<VoicePlaybackStatus, 'idle'>;
    },
  ) => void;
  updatePlaybackStatus: (
    status: Exclude<VoicePlaybackStatus, 'idle'>,
  ) => void;
  clearPlayback: () => void;
  clearPendingPlayback: () => void;
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
export const useVoicePlaybackStore = create<VoiceState>((set) => ({
  activePlayback: IDLE_PLAYBACK,
  pendingPlayback: null,
  setActivePlayback: (payload) =>
    set({
      activePlayback: {
        ...payload,
        status: payload.status ?? 'playing',
      },
    }),
  // pendingPlayback 只表示“等当前播放彻底结束后要接着播的目标”，
  // 不代表当前已经开始输出音频，因此不能直接替代 activePlayback。
  setPendingPlayback: (payload) =>
    set({
      pendingPlayback: {
        ...payload,
        status: payload.status ?? 'loading',
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
  clearPendingPlayback: () => set({ pendingPlayback: null }),
}));
