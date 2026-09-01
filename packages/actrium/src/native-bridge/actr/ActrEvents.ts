import { NativeEventEmitter, NativeModules } from 'react-native';
import { ACTR_BRIDGE_CONTRACT } from '../../actr-host/coreContract';

const ActrBridge = NativeModules[ACTR_BRIDGE_CONTRACT.moduleName];
const eventEmitter = new NativeEventEmitter(ActrBridge);

export type ActrStreamMessageType =
  (typeof ACTR_BRIDGE_CONTRACT.streamTypes)[keyof typeof ACTR_BRIDGE_CONTRACT.streamTypes];

export type ActrStreamMessageEvent = {
  streamID: string;
  message?: string;
  status: number;
  type?: ActrStreamMessageType;
  id?: string;
  statusCode?: number;
  errorMessage?: string;
  metadata?: { value: string; key: string }[];
  sequence?: number; // Speech Pack Order
  timestampMs?: number;
};

type Unsubscribe = () => void;

export const ActrEvents = {
  onStreamMessage: (
    callback: (event: ActrStreamMessageEvent) => void,
  ): Unsubscribe => {
    const subscription = eventEmitter.addListener(
      ACTR_BRIDGE_CONTRACT.streamEventName,
      callback,
    );
    return () => subscription.remove();
  },
};
