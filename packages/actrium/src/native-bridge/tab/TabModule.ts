import { NativeModules } from 'react-native';

const { TabModule } = NativeModules;

export interface TabSummary {
  tabId: string;
  title: string;
  icon: string;
  appUrl: string;
  status?: 'alive' | 'dead';
}

// Open tab(for phone)
export const openTab = async (data: {
  tabId?: string;
  appUrl?: string;
  systemAppType?: string; // Universal systems app(No tab Medium)
  systemAppPayload?: Record<string, unknown>;
}): Promise<{ tabId: string }> => {
  try {
    return TabModule.openTab(data);
  } catch (e) {
    console.error(e);
  }
  return { tabId: '' };
};

// Select Assign tab(for pad)
export const selectTab = async (tabId: string): Promise<boolean> => {
  try {
    return TabModule.selectTab(tabId);
  } catch (e) {
    console.error(e);
  }
  return false;
};

// Close Aspect tab
export const closeTab = async (tabId: string): Promise<boolean> => {
  try {
    return TabModule.closeTab(tabId);
  } catch (e) {
    console.error(e);
  }
  return false;
};

// Exit tab
export const exitFullscreen = async (): Promise<boolean> => {
  try {
    return TabModule.exitFullscreen();
  } catch (e) {
    console.error(e);
  }
  return false;
};

// Quit Search
export const exitSearch = async (): Promise<boolean> => {
  try {
    return TabModule.exitSearch();
  } catch (e) {
    console.error(e);
  }
  return false;
};

// Settings tab Unread State
export const setTabUnreadStatus = async (
  tabId: string,
  unread: boolean,
): Promise<boolean> => {
  try {
    return TabModule.setTabUnreadStatus(tabId, unread);
  } catch (e) {
    console.error(e);
  }
  return false;
};

// Fetch the currently opened tab id
export const getCurrentFullscreenTab = async (): Promise<string> => {
  try {
    return TabModule.getCurrentFullscreenTab();
  } catch (e) {
    console.error(e);
  }
  return '';
};

// Fetches all current tab Basic information
export const getAllTabs = async (): Promise<TabSummary[]> => {
  try {
    return TabModule.getAllTabs();
  } catch (e) {
    console.error(e);
  }
  return [];
};

// Settings tab Collection Status
export const setTabBookmarkStatus = async (
  tabId: string,
  isBookmarked: boolean,
): Promise<boolean> => {
  try {
    return TabModule.setTabBookmarkStatus(tabId, isBookmarked);
  } catch (e) {
    console.error(e);
  }
  return false;
};

// Settings tab Information
export const updateTabInfo = async ({
  tabId,
  title,
  icon,
  appUrl,
  status = 'alive',
}: {
  tabId: string;
  title: string;
  icon: string;
  appUrl: string;
  status?: 'alive' | 'dead';
}): Promise<boolean> => {
  try {
    return TabModule.updateTabInfo({ tabId, title, icon, appUrl, status });
  } catch (e) {
    console.error(e);
  }
  return false;
};
