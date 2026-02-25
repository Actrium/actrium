import { create } from 'zustand';

interface currentAppState {
  // Key is tabId, value is appId
  tabIdMap: Record<string, string>;
  // Key is appId, value is appUrl
  appUrls: Record<string, string>;
  setTabIdMap: (tabId: string, appId: string, url?: string) => void;
  removeTabIdMap: (tabId: string) => void;
}

// tab_id -> app_id
export const useTabIdMapStore = create<currentAppState>((set) => ({
  tabIdMap: {},
  appUrls: {},
  setTabIdMap: (tabId, appId, url) =>
    set((state) => ({
      tabIdMap: { ...state.tabIdMap, [tabId]: appId },
      appUrls: url ? { ...state.appUrls, [appId]: url } : state.appUrls,
    })),
  removeTabIdMap: (tabId) =>
    set((state) => {
      const newTabIdMap = { ...state.tabIdMap };
      const appId = newTabIdMap[tabId];
      delete newTabIdMap[tabId];
      
      // We keep appUrls for now as multiple tabs might point to the same app
      return { tabIdMap: newTabIdMap };
    }),
}));
