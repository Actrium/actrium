import { NativeModules } from 'react-native';
import { ACTR_BRIDGE_CONTRACT } from '../../actr-host/coreContract';

const ActrBridge = NativeModules[ACTR_BRIDGE_CONTRACT.moduleName];

// Unified entry students Actr Capability, avoid a direct reliance on top logic NativeModules.
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
  prepareAttachmentUpload: (params: {
    tabId: string;
    id: string;
    filename: string;
    type: number;
    streamId: string;
    checksum: string;
    checksumAlgorithm: string;
    sizeBytes: number;
    resultStreamId: string;
  }) => Promise<{ id: string; errorMessage: string; statusCode: number }>;
  sendAttachmentChunk: (
    params: {
      tabId: string;
      sessionId: string;
      attachmentId: string;
      chunkId: string;
      streamId: string;
      metadata: { key: string; value: string }[];
    },
    chunkData: string,
  ) => Promise<{ streamID: string; status: number; message: string }>;
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
