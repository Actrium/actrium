import { NativeEventEmitter, NativeModules } from 'react-native';

const { ActrBridge } = NativeModules;
const eventEmitter = new NativeEventEmitter(ActrBridge);

export type ActrStreamMessageEvent = {
  streamID: string;
  message: string;
  status: number;
  metadata?: { value: string; key: string }[];
};

type Unsubscribe = () => void;

export const ActrEvents = {
  onStreamMessage: (
    callback: (event: ActrStreamMessageEvent) => void,
  ): Unsubscribe => {
    const subscription = eventEmitter.addListener(
      'ActrStreamMessage',
      callback,
    );
    return () => subscription.remove();
  },
};
