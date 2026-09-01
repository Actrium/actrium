import { NativeEventEmitter, NativeModules } from 'react-native';
import { RUNTIME_NATIVE_EVENTS } from '../../runtime-host/coreContract';

const { ExtensionEventEmitter } = NativeModules;
const eventEmitter = new NativeEventEmitter(ExtensionEventEmitter);

export type ExtensionPanelVisibleChangeEvent = {
  visible: boolean;
  tabId: string;
};

type Unsubscribe = () => void;

export const ExtensionEvents = {
  // Listen to the change in status of the original side extension panel for business-level synchronization UI Status.
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
