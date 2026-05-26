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

export type TAttachRequest = {
  id: string;
  filename: string;
  type: AttachmentType;
  data: Uint8Array;
};

export type TAttachResponse = {
  id: string;
  errorMessage: string;
  statusCode: number;
};

export type TLocation = {
  latitude: number;
  longitude: number;
  address: string;
  placeName: string;
};
