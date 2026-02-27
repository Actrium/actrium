import { NativeModules } from 'react-native';

const { TabModule, ActrBridge } = NativeModules;

// 关闭 tab
export const closeCurrentTab = async (tabId: string): Promise<boolean> => {
  return TabModule.closeTab(tabId);
};

export const openTab = async (options: {
  systemAppType: string;
}): Promise<string> => {
  return TabModule.openTab(options);
};

export const selectTab = async (tabId: string): Promise<boolean> => {
  return TabModule.selectTab(tabId);
};

// 退出 tab
export const exitFullscreen = async (): Promise<boolean> => {
  return ActrBridge.exitFullscreen();
};
