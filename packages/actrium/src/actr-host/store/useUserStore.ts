import { create } from 'zustand';
import { AccountUserInfo } from '../../api/types';

interface UserState {
  userInfo: AccountUserInfo | null;
  sessionToken: string | null;
  setUserInfo: (info: AccountUserInfo | null) => void;
  setSessionToken: (token: string | null) => void;
  clearUser: () => void;
}

/**
 * Unified user information Store - Only store from Auth API Data obtained
 */
export const useUserStore = create<UserState>((set) => ({
  userInfo: null,
  sessionToken: null,
  setUserInfo: (info) => set({ userInfo: info }),
  setSessionToken: (token) => set({ sessionToken: token }),
  clearUser: () => set({ userInfo: null, sessionToken: null }),
}));
