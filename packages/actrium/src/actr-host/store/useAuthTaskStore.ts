import { create } from 'zustand';
import {
  AUTH_ACCESS_CONTROL_RESPONSE_CONTRACT,
  type AuthAccessControlStatus,
} from '../coreContract';

export type AppAuthorizationStatus = AuthAccessControlStatus;

interface AuthTaskState {
  // Here. appId Yes key, Cache Mandated State; default considered unknown
  alreadyAuthorizedAppIds: Record<string, AppAuthorizationStatus>;
  addAuthorizedAppId: (appId: string) => void;
  addUnauthorizedAppId: (appId: string) => void;
  getAppAuthorizationStatus: (appId: string) => AppAuthorizationStatus;
  clearAuthorizationCache: () => void;
}

/**
 * To manage the application authorization cache.
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
      // Records“Unauthorized confirmed”Avoid requesting a request for a status interface every time you send
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
