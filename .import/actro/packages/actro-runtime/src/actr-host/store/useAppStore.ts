import { create } from 'zustand';
import { AppInfo } from '../../api/types';
import { AppRepository } from '../../repository/AppRepository';

interface AppState {
  // Key is appId
  apps: Record<string, AppInfo>;
  // 🆕 Key is appId, value is guest script content
  guestScripts: Record<string, string>;
  // key is appId, guest loading status
  guestLoadingStatus: Record<string, boolean>;

  setAppInfo: (appId: string, info: AppInfo) => void;
  updateAppStatus: (
    appId: string,
    status: 'alive' | 'dead',
  ) => Promise<void>;
  setGuestScript: (appId: string, script: string) => void;
  getGuestScript: (appId: string) => string | undefined;
  setGuestLoadingStatus: (appId: string, status: boolean) => void;
  cleanAppInfo: (appId: string) => void;
  clearAll: () => void;
}

export const useAppStore = create<AppState>((set, get) => ({
  apps: {},
  guestScripts: {},
  guestLoadingStatus: {},

  setAppInfo: (appId, info) =>
    set((state) => ({
      apps: { ...state.apps, [appId]: info },
    })),

  updateAppStatus: async (appId, status) => {
    const hasDbRecord = await AppRepository.updateAppStatus(appId, status);
    const currentApp = get().apps[appId];

    if (!currentApp && !hasDbRecord) {
      return;
    }

    if (!currentApp) {
      return;
    }

    set((state) => ({
      apps: {
        ...state.apps,
        [appId]: {
          ...state.apps[appId],
          app_status: status,
        },
      },
    }));
  },

  setGuestScript: (appId, script) =>
    set((state) => ({
      guestScripts: { ...state.guestScripts, [appId]: script },
    })),

  getGuestScript: (appId) => get().guestScripts[appId],

  setGuestLoadingStatus: (appId: string, status: boolean) =>
    set((state) => ({
      guestLoadingStatus: { ...state.guestLoadingStatus, [appId]: status },
    })),

  cleanAppInfo: (appId) =>
    set((state) => {
      const newApps = { ...state.apps };
      const newScripts = { ...state.guestScripts };
      delete newApps[appId];
      delete newScripts[appId];
      return { apps: newApps, guestScripts: newScripts };
    }),

  clearAll: () => set({ apps: {}, guestScripts: {} }),
}));
