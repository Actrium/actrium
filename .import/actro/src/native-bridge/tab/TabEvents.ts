import { NativeModules, NativeEventEmitter } from 'react-native';

const { TabEventEmitter } = NativeModules;
const eventEmitter = new NativeEventEmitter(TabEventEmitter);

export type TabFullscreenEvent = {
  tabId: string;
  title?: string;
  timestamp: number;
};

export type TabUnreadStatusEvent = {
  tabId: string;
  unread: boolean;
};

type Unsubscribe = () => void;

export const TabEvents = {
  // 进入 tab 事件
  onTabEnteredFullscreen: (
    callback: (event: TabFullscreenEvent) => void,
  ): Unsubscribe => {
    const subscription = eventEmitter.addListener(
      'onTabEnteredFullscreen',
      callback,
    );
    return () => subscription.remove();
  },

  // 退出 tab 事件
  onTabExitedFullscreen: (
    callback: (event: TabFullscreenEvent) => void,
  ): Unsubscribe => {
    const subscription = eventEmitter.addListener(
      'onTabExitedFullscreen',
      callback,
    );
    return () => subscription.remove();
  },
};
