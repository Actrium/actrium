import { NativeModules } from 'react-native';

const { TabModule } = NativeModules;

export interface TabSummary {
  tabId: string;
  title: string;
  icon: string;
  appUrl: string;
  status?: 'alive' | 'dead';
}

// 打开 tab（for phone）
export const openTab = async (data: {
  tabId?: string;
  appUrl?: string;
  systemAppType?: string; // 通用的系统 app（非 tab 中）
  systemAppPayload?: Record<string, unknown>;
}): Promise<{ tabId: string }> => {
  try {
    return TabModule.openTab(data);
  } catch (e) {
    console.error(e);
  }
  return { tabId: '' };
};

// 选中指定 tab（for pad）
export const selectTab = async (tabId: string): Promise<boolean> => {
  try {
    return TabModule.selectTab(tabId);
  } catch (e) {
    console.error(e);
  }
  return false;
};

// 关闭指定 tab
export const closeTab = async (tabId: string): Promise<boolean> => {
  try {
    return TabModule.closeTab(tabId);
  } catch (e) {
    console.error(e);
  }
  return false;
};

// 退出 tab
export const exitFullscreen = async (): Promise<boolean> => {
  try {
    return TabModule.exitFullscreen();
  } catch (e) {
    console.error(e);
  }
  return false;
};

// 退出搜索
export const exitSearch = async (): Promise<boolean> => {
  try {
    return TabModule.exitSearch();
  } catch (e) {
    console.error(e);
  }
  return false;
};

// 设置 tab 未读状态
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

// 获取当前打开的 tab id
export const getCurrentFullscreenTab = async (): Promise<string> => {
  try {
    return TabModule.getCurrentFullscreenTab();
  } catch (e) {
    console.error(e);
  }
  return '';
};

// 获取当前所有 tab 的基础信息
export const getAllTabs = async (): Promise<TabSummary[]> => {
  try {
    return TabModule.getAllTabs();
  } catch (e) {
    console.error(e);
  }
  return [];
};

// 设置 tab 收藏状态
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

// 设置 tab 信息
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
