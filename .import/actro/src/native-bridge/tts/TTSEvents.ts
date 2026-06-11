import { NativeEventEmitter, NativeModules, Platform } from 'react-native';

const { TTSModule } = NativeModules;

type Unsubscribe = () => void;

export type TTSEventPayload = Record<string, unknown>;

export interface TTSFinishEvent extends TTSEventPayload {
  cancelled?: boolean;
}

function getEventEmitter(): NativeEventEmitter {
  if (Platform.OS !== 'ios') {
    throw new Error('TTSEvents is only supported on iOS');
  }

  if (!TTSModule) {
    throw new Error('TTSModule is not available');
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
    addListener('onTTSStart', callback),

  onTTSPause: (callback: (event: TTSEventPayload) => void): Unsubscribe =>
    addListener('onTTSPause', callback),

  onTTSResume: (callback: (event: TTSEventPayload) => void): Unsubscribe =>
    addListener('onTTSResume', callback),

  // finish 事件需要保留 cancelled 标记，供上层区分自然播放完成与主动 stop/cancel。
  onTTSFinish: (callback: (event: TTSFinishEvent) => void): Unsubscribe =>
    addListener('onTTSFinish', callback),

  onTTSError: (callback: (event: TTSEventPayload) => void): Unsubscribe =>
    addListener('onTTSError', callback),
};
