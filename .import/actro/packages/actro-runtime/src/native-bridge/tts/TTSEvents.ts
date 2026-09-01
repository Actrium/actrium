import { NativeEventEmitter, NativeModules, Platform } from 'react-native';
import { TTS_BRIDGE_CONTRACT } from '../../actr-host/coreContract';

const TTSModule = NativeModules[TTS_BRIDGE_CONTRACT.moduleName];

type Unsubscribe = () => void;

export type TTSEventPayload = Record<string, unknown>;

export type TTSFinishEvent = TTSEventPayload &
  Partial<{
    [TTS_BRIDGE_CONTRACT.payloadKeys.cancelled]: boolean;
  }>;

function getEventEmitter(): NativeEventEmitter {
  if (Platform.OS !== 'ios') {
    throw new Error('TTSEvents is only supported on iOS');
  }

  if (!TTSModule) {
    throw new Error(`${TTS_BRIDGE_CONTRACT.moduleName} is not available`);
  }

  return new NativeEventEmitter(TTSModule);
}

function addListener<TEvent extends TTSEventPayload>(
  eventName: string,
  callback: (event: TEvent) => void,
): Unsubscribe {
  const subscription = getEventEmitter().addListener(eventName, callback);
  return () => subscription.remove();
}

export const TTSEvents = {
  onTTSStart: (callback: (event: TTSEventPayload) => void): Unsubscribe =>
    addListener(TTS_BRIDGE_CONTRACT.events.start, callback),

  onTTSPause: (callback: (event: TTSEventPayload) => void): Unsubscribe =>
    addListener(TTS_BRIDGE_CONTRACT.events.pause, callback),

  onTTSResume: (callback: (event: TTSEventPayload) => void): Unsubscribe =>
    addListener(TTS_BRIDGE_CONTRACT.events.resume, callback),

  // finish 事件需要保留 cancelled 标记，供上层区分自然播放完成与主动 stop/cancel。
  onTTSFinish: (callback: (event: TTSFinishEvent) => void): Unsubscribe =>
    addListener(TTS_BRIDGE_CONTRACT.events.finish, callback),

  onTTSError: (callback: (event: TTSEventPayload) => void): Unsubscribe =>
    addListener(TTS_BRIDGE_CONTRACT.events.error, callback),
};
