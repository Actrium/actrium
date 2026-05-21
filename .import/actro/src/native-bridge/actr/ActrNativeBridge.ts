import { NativeModules } from 'react-native';

const { ActrBridge } = NativeModules;

// 统一收口原生 Actr 能力，避免上层逻辑直接依赖 NativeModules。
export const ActrNativeBridge = ActrBridge as {
  initializeService: (params: {
    path: string;
    actorType: string;
    tabId: string;
  }) => Promise<void>;
  sendTextMessage: (params: Record<string, unknown>) => Promise<void>;
  startVoice: (params: Record<string, unknown>) => Promise<{ streamId: string }>;
  sendVoiceChunk: (
    params: Record<string, unknown>,
    data: string,
  ) => Promise<string>;
  uploadAttachment: (params: {
    streamId: string;
    type: number;
    fileData: string;
    actorType: string;
    tabId: string;
    filename: string;
  }) => Promise<{ id: string; errorMessage: string; statusCode: number }>;
  shutdownService: (params: {
    actorType: string;
    tabId: string;
  }) => Promise<void>;
  unregisterStream: (params: {
    streamId: string;
    actorType: string;
    tabId: string;
  }) => Promise<unknown>;
};
