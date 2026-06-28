import { AppInfo } from '../../api/types';

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

export enum AttachmentType {
  ATTACHMENT_TYPE_UNKNOWN = 0,
  IMAGE = 1,
  DOCUMENT = 2,
  AUDIO = 3,
  VIDEO = 4,
  OTHER = 99,
}

export enum AttachmentUploadStatus {
  UNKNOWN = 0,
  COMPLETED = 1,
  FAILED = 2,
}

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
