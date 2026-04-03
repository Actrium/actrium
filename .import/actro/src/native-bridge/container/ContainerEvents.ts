import { NativeEventEmitter, NativeModules } from 'react-native';

export type ContainerSizeChangeEvent = {
  containerWidth: number;
  containerHeight: number;
};

type Unsubscribe = () => void;

const getContainerEventEmitter = () => {
  const { ExtensionEventEmitter } = NativeModules;
  return ExtensionEventEmitter
    ? new NativeEventEmitter(ExtensionEventEmitter)
    : null;
};

export const ContainerEvents = {
  // 监听宿主容器尺寸变化，供安卓宿主页在 RootView 尺寸变化后同步最新宽高。
  onContainerSizeChange: (
    callback: (event: ContainerSizeChangeEvent) => void,
  ): Unsubscribe => {
    const eventEmitter = getContainerEventEmitter();
    if (!eventEmitter) {
      return () => {};
    }

    const subscription = eventEmitter.addListener(
      'onContainerSizeChange',
      callback,
    );
    return () => subscription.remove();
  },
};
