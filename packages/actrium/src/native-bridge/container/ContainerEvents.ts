import { NativeEventEmitter, NativeModules } from 'react-native';
import { RUNTIME_NATIVE_EVENTS } from '../../runtime-host/coreContract';

const { ExtensionEventEmitter } = NativeModules;
const eventEmitter = new NativeEventEmitter(ExtensionEventEmitter);

export type ContainerSizeChangeEvent = {
  containerWidth: number;
  containerHeight: number;
};

type Unsubscribe = () => void;

export const ContainerEvents = {
  // Listen to the changes in the size of the host container for the Andreau Home Page RootView Synchronizes the latest width and heights after a change in size.
  // only by android
  onContainerSizeChange: (
    callback: (event: ContainerSizeChangeEvent) => void,
  ): Unsubscribe => {
    const subscription = eventEmitter.addListener(
      RUNTIME_NATIVE_EVENTS.containerSizeChange,
      callback,
    );
    return () => subscription.remove();
  },
};
