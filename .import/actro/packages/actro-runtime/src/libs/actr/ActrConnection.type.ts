import { AppInfo } from '../../api/types';
import { ACTR_BRIDGE_CONTRACT } from '../../actr-host/coreContract';

export interface IActrConfigOptions {
  app_id: string;
  key_id: string;
  authority_services: AppInfo['authority_services'];
  signaling_services: AppInfo['signaling_services'];
  stun_services: AppInfo['stun_services'];
  turn_services: AppInfo['turn_services'];
}

export interface TActrMessageRequest {
  messageId: string;
  sessionId: string;
  text: string;
  location?: TLocation;
  attachmentIds?: string[];
}

export interface TActrVoiceRequest {
  messageId: string;
  sessionId: string;
  location?: TLocation;
  attachmentIds?: string[];
}

export type TActrMessageResponse = {
  message_id: string;
  message: string | Uint8Array;
  status: number;
  isEnd: boolean;
  isAbort: boolean;
  sequence?: number;
};

export const AttachmentType = {
  ATTACHMENT_TYPE_UNKNOWN: ACTR_BRIDGE_CONTRACT.attachment.types.unknown,
  IMAGE: ACTR_BRIDGE_CONTRACT.attachment.types.image,
  DOCUMENT: ACTR_BRIDGE_CONTRACT.attachment.types.document,
  AUDIO: ACTR_BRIDGE_CONTRACT.attachment.types.audio,
  VIDEO: ACTR_BRIDGE_CONTRACT.attachment.types.video,
  OTHER: ACTR_BRIDGE_CONTRACT.attachment.types.other,
} as const;
export type AttachmentType =
  (typeof AttachmentType)[keyof typeof AttachmentType];

export const AttachmentUploadStatus = {
  UNKNOWN: ACTR_BRIDGE_CONTRACT.attachment.uploadStatus.unknown,
  COMPLETED: ACTR_BRIDGE_CONTRACT.attachment.uploadStatus.completed,
  FAILED: ACTR_BRIDGE_CONTRACT.attachment.uploadStatus.failed,
} as const;
export type AttachmentUploadStatus =
  (typeof AttachmentUploadStatus)[keyof typeof AttachmentUploadStatus];

export type TAttachmentUploadRequest = {
  attachmentId: string;
  sessionId: string;
  fileName: string;
  filePath: string;
};

export type TAttachmentUploadResult = {
  id: string;
  status: AttachmentUploadStatus;
  errorMessage: string;
  statusCode: number;
  metadata?: { value: string; key: string }[];
  sequence?: number;
  timestampMs?: number;
};

export type TLocation = {
  latitude: number;
  longitude: number;
  address: string;
  placeName: string;
};
