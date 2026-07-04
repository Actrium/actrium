import { create } from 'zustand';
import {
  AUTH_ACCESS_CONTROL_RESPONSE_CONTRACT,
  type AuthAccessControlStatus,
} from '../coreContract';

export type AppAuthorizationStatus = AuthAccessControlStatus;

interface AuthTaskState {
  // 以 appId 为 key，缓存授权状态；缺省视为 unknown
  alreadyAuthorizedAppIds: Record<string, AppAuthorizationStatus>;
  addAuthorizedAppId: (appId: string) => void;
  addUnauthorizedAppId: (appId: string) => void;
  getAppAuthorizationStatus: (appId: string) => AppAuthorizationStatus;
  clearAuthorizationCache: () => void;
}

/**
 * 用于管理应用授权状态缓存。
 */
export const useAuthTaskStore = create<AuthTaskState>((set, get) => ({
  alreadyAuthorizedAppIds: {},
  addAuthorizedAppId: (appId) =>
    set((state) => {
      return {
        alreadyAuthorizedAppIds: {
          ...state.alreadyAuthorizedAppIds,
          [appId]: AUTH_ACCESS_CONTROL_RESPONSE_CONTRACT.statuses.authorized,
        },
      };
    }),
  addUnauthorizedAppId: (appId) =>
    set((state) => {
      // 记录“已确认未授权”，避免每次发送都重复请求授权状态接口
      return {
        alreadyAuthorizedAppIds: {
          ...state.alreadyAuthorizedAppIds,
          [appId]: AUTH_ACCESS_CONTROL_RESPONSE_CONTRACT.statuses.notAuthorized,
        },
      };
    }),
  getAppAuthorizationStatus: (appId) =>
    get().alreadyAuthorizedAppIds[appId] ||
    AUTH_ACCESS_CONTROL_RESPONSE_CONTRACT.statuses.unknown,
  clearAuthorizationCache: () =>
    set(() => ({
      alreadyAuthorizedAppIds: {},
    })),
}));
