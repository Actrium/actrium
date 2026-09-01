import { Platform } from 'react-native';

const speechRecognitionModule =
  Platform.OS === 'android'
    ? require('./SpeechRecognitionService.android')
    : require('./SpeechRecognitionService.ios');

export const SpeechRecognitionService =
  speechRecognitionModule.SpeechRecognitionService as typeof import('./index.ios').SpeechRecognitionService;

export const speechRecognitionService =
  speechRecognitionModule.speechRecognitionService as InstanceType<
    typeof import('./index.ios').SpeechRecognitionService
  >;
