import { NativeEventEmitter, NativeModules } from 'react-native';
import { RUNTIME_NATIVE_EVENTS } from '../../actr-host/coreContract';

const { ExtensionEventEmitter } = NativeModules;
const eventEmitter = new NativeEventEmitter(ExtensionEventEmitter);

export type ExtensionPanelVisibleChangeEvent = {
  visible: boolean;
  tabId: string;
};

type Unsubscribe = () => void;

export const ExtensionEvents = {
  // 监听原生侧扩展面板显示状态变化，供业务层同步 UI 状态。
  // only by android
  onExtensionPanelVisibleChange: (
    callback: (event: ExtensionPanelVisibleChangeEvent) => void,
  ): Unsubscribe => {
    const subscription = eventEmitter.addListener(
      RUNTIME_NATIVE_EVENTS.extensionPanelVisibleChange,
      callback,
    );
    return () => subscription.remove();
  },
};
