import { NativeModules } from 'react-native';

const { TabModule } = NativeModules;

// 打开 tab
export const openTab = async (data: {
  tabId: string;
  appUrl: string;
}): Promise<string> => {
  try {
    return TabModule.openTab(data);
  } catch (e) {
    console.error(e);
  }
  return '';
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
  url,
}: {
  tabId: string;
  title: string;
  icon: string;
  url: string;
}): Promise<boolean> => {
  try {
    return TabModule.updateTabInfo({ tabId, title, icon, url });
  } catch (e) {
    console.error(e);
  }
  return false;
};
