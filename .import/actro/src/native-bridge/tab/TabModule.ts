import { NativeModules } from 'react-native';

const { TabModule } = NativeModules;

// 关闭 tab
export const closeCurrentTab = async (tabId: string): Promise<boolean> => {
  try {
    return TabModule.closeTab(tabId);
  } catch (e) {
    console.error(e);
  }
  return false;
};

export const openTab = async (data: {
  tabId: string;
  openUrl: string;
}): Promise<string> => {
  try {
    return TabModule.openTab(data);
  } catch (e) {
    console.error(e);
  }
  return '';
};

export const selectTab = async (tabId: string): Promise<boolean> => {
  try {
    return TabModule.selectTab(tabId);
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
