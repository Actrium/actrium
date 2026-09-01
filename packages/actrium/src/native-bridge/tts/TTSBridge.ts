import { NativeModules, Platform } from 'react-native';
import { TTS_BRIDGE_CONTRACT } from '../../runtime-host/coreContract';

const TTSModule = NativeModules[TTS_BRIDGE_CONTRACT.moduleName];

export type TTSSpeakOptions = Partial<{
  [TTS_BRIDGE_CONTRACT.optionKeys.rate]: number;
  [TTS_BRIDGE_CONTRACT.optionKeys.pitchMultiplier]: number;
  [TTS_BRIDGE_CONTRACT.optionKeys.volume]: number;
  [TTS_BRIDGE_CONTRACT.optionKeys.language]: string;
  [TTS_BRIDGE_CONTRACT.optionKeys.voiceIdentifier]: string;
}>;

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
    throw new Error(`${TTS_BRIDGE_CONTRACT.moduleName} is not available`);
  }

  return ttsModule;
}

export const speak = async (
  text: string,
  options: TTSSpeakOptions = {},
): Promise<void> => {
  const module = getTTSModule('speak');

  if (!module.speak) {
    throw new Error(`${TTS_BRIDGE_CONTRACT.moduleName}.speak is not available`);
  }

  await module.speak(text, options);
};

export const pause = async (): Promise<void> => {
  const module = getTTSModule('pause');

  if (!module.pause) {
    throw new Error(`${TTS_BRIDGE_CONTRACT.moduleName}.pause is not available`);
  }

  await module.pause();
};

export const resume = async (): Promise<void> => {
  const module = getTTSModule('resume');

  if (!module.resume) {
    throw new Error(`${TTS_BRIDGE_CONTRACT.moduleName}.resume is not available`);
  }

  await module.resume();
};

export const stop = async (): Promise<void> => {
  const module = getTTSModule('stop');

  if (!module.stop) {
    throw new Error(`${TTS_BRIDGE_CONTRACT.moduleName}.stop is not available`);
  }

  await module.stop();
};

export const getAvailableLanguages = async (): Promise<string[]> => {
  const module = getTTSModule('getAvailableLanguages');

  if (!module.getAvailableLanguages) {
    throw new Error(
      `${TTS_BRIDGE_CONTRACT.moduleName}.getAvailableLanguages is not available`,
    );
  }

  return await module.getAvailableLanguages();
};
