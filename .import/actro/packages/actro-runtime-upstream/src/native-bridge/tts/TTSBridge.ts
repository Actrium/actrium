import { NativeModules, Platform } from 'react-native';

const { TTSModule } = NativeModules;

export interface TTSSpeakOptions {
  rate?: number;
  pitchMultiplier?: number;
  volume?: number;
  language?: string;
  voiceIdentifier?: string;
}

interface TTSModuleLike {
  speak?: (text: string, options?: TTSSpeakOptions) => Promise<void> | void;
  pause?: () => Promise<void> | void;
  resume?: () => Promise<void> | void;
  stop?: () => Promise<void> | void;
  getAvailableLanguages?: () => Promise<string[]> | string[];
}

const ttsModule = TTSModule as TTSModuleLike | undefined;

function getTTSModule(methodName: string): TTSModuleLike {
  if (Platform.OS !== 'ios') {
    throw new Error(`TTSBridge.${methodName} is only supported on iOS`);
  }

  if (!ttsModule) {
    throw new Error('TTSModule is not available');
  }

  return ttsModule;
}

export const speak = async (
  text: string,
  options: TTSSpeakOptions = {},
): Promise<void> => {
  const module = getTTSModule('speak');

  if (!module.speak) {
    throw new Error('TTSModule.speak is not available');
  }

  await module.speak(text, options);
};

export const pause = async (): Promise<void> => {
  const module = getTTSModule('pause');

  if (!module.pause) {
    throw new Error('TTSModule.pause is not available');
  }

  await module.pause();
};

export const resume = async (): Promise<void> => {
  const module = getTTSModule('resume');

  if (!module.resume) {
    throw new Error('TTSModule.resume is not available');
  }

  await module.resume();
};

export const stop = async (): Promise<void> => {
  const module = getTTSModule('stop');

  if (!module.stop) {
    throw new Error('TTSModule.stop is not available');
  }

  await module.stop();
};

export const getAvailableLanguages = async (): Promise<string[]> => {
  const module = getTTSModule('getAvailableLanguages');

  if (!module.getAvailableLanguages) {
    throw new Error('TTSModule.getAvailableLanguages is not available');
  }

  return await module.getAvailableLanguages();
};
