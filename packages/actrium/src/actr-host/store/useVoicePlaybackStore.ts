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
  // Speak Mode Switch to pending After that, the target text needs to be retained, and it needs to be preserved. finish I'll be back when the events come back. speak.
  text?: string;
  // Speed, high-read parameters to cache and target, avoid stop -> finish . The configuration is lost.
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
 * Global voice play state management
 * Chinese Comment: Unified maintenance“Which part of the message is being played in what mode and at the current stage”,
 * Avoid question / answer Share one. messageId .
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
  // pendingPlayback Only“The target to be followed when the current play is over.”,
  // It does not mean that currently the output of audio has started, so it cannot be replaced directly. activePlayback.
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
