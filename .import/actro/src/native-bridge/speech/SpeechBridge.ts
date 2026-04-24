import { NativeModules } from 'react-native';

const { AsrModule } = NativeModules;

interface AsrModuleLike {
  setLanguage?: (language: string) => Promise<void> | void;
  feedAudio?: (audioData: string) => Promise<void> | void;
  stop?: () => Promise<string> | string;
}

const asrModule = AsrModule as AsrModuleLike | undefined;

export const setLanguage = async (language: string): Promise<void> => {
  if (!asrModule?.setLanguage) {
    throw new Error('AsrModule.setLanguage is not available');
  }

  await asrModule.setLanguage(language);
};

export const feedAudio = async (audioData: string): Promise<void> => {
  if (!asrModule?.feedAudio) {
    throw new Error('AsrModule.feedAudio is not available');
  }

  await asrModule.feedAudio(audioData);
};

export const stop = async (): Promise<string> => {
  if (!asrModule?.stop) {
    throw new Error('AsrModule.stop is not available');
  }

  return await asrModule.stop();
};
