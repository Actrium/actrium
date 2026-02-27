import { NativeModules, NativeEventEmitter } from 'react-native';

const { TabEventEmitter } = NativeModules;
const eventEmitter = new NativeEventEmitter(TabEventEmitter);

export type TabFullscreenEvent = {
  tabId: string;
  title?: string;
  timestamp: number;
};

type Unsubscribe = () => void;

export const TabEvents = {
  onTabEnteredFullscreen: (
    callback: (event: TabFullscreenEvent) => void,
  ): Unsubscribe => {
    const subscription = eventEmitter.addListener(
      'onTabEnteredFullscreen',
      callback,
    );
    return () => subscription.remove();
  },

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
