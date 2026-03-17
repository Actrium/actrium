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

export type TabBookmarkEvent = {
  tabId: string;
  isBookmarked: boolean;
};

type Unsubscribe = () => void;
type LocalTabExitListener = (event: TabFullscreenEvent) => void;

const willExitFullscreenListeners = new Set<LocalTabExitListener>();

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

  // native 端设置收藏状态
  onTabBookmarkChanged: (
    callback: (event: TabBookmarkEvent) => void,
  ): Unsubscribe => {
    const subscription = eventEmitter.addListener(
      'onTabBookmarkChanged',
      callback,
    );
    return () => subscription.remove();
  },

  // 自定义
  // JS 侧主动触发退出前清理，确保在 Native 切走页面前就能释放相机等资源。
  onTabWillExitFullscreen: (
    callback: (event: TabFullscreenEvent) => void,
  ): Unsubscribe => {
    willExitFullscreenListeners.add(callback);
    return () => {
      willExitFullscreenListeners.delete(callback);
    };
  },
};

export const emitTabWillExitFullscreen = (event: TabFullscreenEvent) => {
  willExitFullscreenListeners.forEach((listener) => {
    try {
      listener(event);
    } catch (error) {
      console.error(
        '[TabEvents] onTabWillExitFullscreen listener failed:',
        error,
      );
    }
  });
};
