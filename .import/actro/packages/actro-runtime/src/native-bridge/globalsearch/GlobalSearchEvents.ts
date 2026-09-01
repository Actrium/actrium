import { NativeEventEmitter, NativeModules } from 'react-native';
import { RUNTIME_NATIVE_EVENTS } from '../../actr-host/coreContract';

const { TabEventEmitter } = NativeModules;

export type SearchInputChangedEvent = {
  inputText: string;
  timestamp?: number;
};

type Unsubscribe = () => void;

function getEventEmitter(): NativeEventEmitter {
  return new NativeEventEmitter(TabEventEmitter);
}

export const GlobalSearchEvents = {
  // 监听宿主搜索输入变化，GlobalSearch 页面只消费宿主传入的输入内容，不再自管输入框状态。
  onSearchInputChanged: (
    callback: (event: SearchInputChangedEvent) => void,
  ): Unsubscribe => {
    const eventEmitter = getEventEmitter();

    const subscription = eventEmitter!.addListener(
      RUNTIME_NATIVE_EVENTS.searchInputChanged,
      callback,
    );
    return () => subscription.remove();
  },
};
