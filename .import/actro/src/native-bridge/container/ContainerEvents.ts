import { NativeEventEmitter, NativeModules } from 'react-native';

const { ExtensionEventEmitter } = NativeModules;
const eventEmitter = new NativeEventEmitter(ExtensionEventEmitter);

export type ContainerSizeChangeEvent = {
  containerWidth: number;
  containerHeight: number;
};

type Unsubscribe = () => void;

export const ContainerEvents = {
  // 监听宿主容器尺寸变化，供安卓宿主页在 RootView 尺寸变化后同步最新宽高。
  // only by android
  onContainerSizeChange: (
    callback: (event: ContainerSizeChangeEvent) => void,
  ): Unsubscribe => {
    const subscription = eventEmitter.addListener(
      'onContainerSizeChange',
      callback,
    );
    return () => subscription.remove();
  },
};
