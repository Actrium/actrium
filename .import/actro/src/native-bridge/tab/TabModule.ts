import { NativeModules } from 'react-native';

const { TabModule } = NativeModules;

// 关闭 tab
export const closeCurrentTab = async (tabId: string): Promise<boolean> => {
  return TabModule.closeTab(tabId);
};

export const openTab = async (data: {
  tabId: string;
  openUrl: string;
}): Promise<string> => {
  return TabModule.openTab(data);
};

export const selectTab = async (tabId: string): Promise<boolean> => {
  return TabModule.selectTab(tabId);
};

// 退出 tab
export const exitFullscreen = async (): Promise<boolean> => {
  try {
    return TabModule.exitFullscreen();
  } catch(e) {
    console.error(e)
  }
  return false
};

// 退出搜索
export const exitSearch = async (): Promise<boolean> => {
  try {
    return TabModule.exitSearch();
  } catch(e) {
    console.error(e)
  }
  return false
};

