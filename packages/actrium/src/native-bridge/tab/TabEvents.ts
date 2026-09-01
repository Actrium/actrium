import { NativeModules, NativeEventEmitter } from 'react-native';
import { RUNTIME_NATIVE_EVENTS } from '../../actr-host/coreContract';

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

export const TabEvents = {
  // Enter tab Events
  onTabEnteredFullscreen: (
    callback: (event: TabFullscreenEvent) => void,
  ): Unsubscribe => {
    const subscription = eventEmitter.addListener(
      RUNTIME_NATIVE_EVENTS.tabEnteredFullscreen,
      callback,
    );
    return () => subscription.remove();
  },

  // Exit tab Events
  onTabExitedFullscreen: (
    callback: (event: TabFullscreenEvent) => void,
  ): Unsubscribe => {
    const subscription = eventEmitter.addListener(
      RUNTIME_NATIVE_EVENTS.tabExitedFullscreen,
      callback,
    );
    return () => subscription.remove();
  },

  // native End Set Collection Status
  onTabBookmarkChanged: (
    callback: (event: TabBookmarkEvent) => void,
  ): Unsubscribe => {
    const subscription = eventEmitter.addListener(
      RUNTIME_NATIVE_EVENTS.tabBookmarkChanged,
      callback,
    );
    return () => subscription.remove();
  },
};
