import { create } from 'zustand';

export type AppAuthorizationStatus =
  | 'unknown'
  | 'authorized'
  | 'not_authorized';

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
          [appId]: 'authorized',
        },
      };
    }),
  addUnauthorizedAppId: (appId) =>
    set((state) => {
      // 记录“已确认未授权”，避免每次发送都重复请求授权状态接口
      return {
        alreadyAuthorizedAppIds: {
          ...state.alreadyAuthorizedAppIds,
          [appId]: 'not_authorized',
        },
      };
    }),
  getAppAuthorizationStatus: (appId) =>
    get().alreadyAuthorizedAppIds[appId] || 'unknown',
  clearAuthorizationCache: () =>
    set(() => ({
      alreadyAuthorizedAppIds: {},
    })),
}));
