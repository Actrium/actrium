import { NativeEventEmitter, NativeModules } from 'react-native';

const { ActrBridge } = NativeModules;
const eventEmitter = new NativeEventEmitter(ActrBridge);

export type ActrStreamMessageEvent = {
  streamID: string;
  message?: string;
  status: number;
  type?: string;
  id?: string;
  statusCode?: number;
  errorMessage?: string;
  metadata?: { value: string; key: string }[];
  sequence?: number; // 语音包顺序
  timestampMs?: number;
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
