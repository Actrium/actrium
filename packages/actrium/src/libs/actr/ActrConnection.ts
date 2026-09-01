import { Buffer } from 'buffer';
import RNFS from 'react-native-fs';
import {
  AttachmentType,
  AttachmentUploadStatus,
  TActrMessageRequest,
  TActrMessageResponse,
  TAttachmentUploadRequest,
  TActrVoiceRequest,
} from './ActrConnection.type';
import { ActrNativeBridge } from '../../native-bridge/actr/ActrNativeBridge';
import { Utils } from '../../utils/utils';
import { ActrStreamRouter, TActrStreamMap } from './ActrStreamRouter';
import { errorNote } from './LocalTestMessage';
import { ACTR_BRIDGE_CONTRACT } from '../../actr-host/coreContract';

// Attachment Upload By 48KB Fixed fractions,RN with native Relying on the promise of all sizes chunk The order is consistent with the boundary.
const ATTACHMENT_CHUNK_SIZE = ACTR_BRIDGE_CONTRACT.attachment.chunkSizeBytes;
const ATTACHMENT_CHECKSUM_ALGORITHM =
  ACTR_BRIDGE_CONTRACT.attachment.checksumAlgorithms.md5;

/**
 * Actr Connection Management Class
 * Each example corresponds to a particular case App - The connection.
 */
export class ActrConnection {
  private appId: string = '';
  private refCount: number = 0;
  private isInitialized: boolean = false;
  private disconnectTimer: any = null;
  private actorType: string = '';
  private tabId: string = '';

  session_id: string = '';
  reason: string = '';

  private currentVoiceRequestStreamId: string = '';
  private currentVoiceChunkCount: number = 0;

  private chatListeners = new Set<(data: TActrMessageResponse) => void>();
  private voiceListeners = new Set<(data: TActrMessageResponse) => void>();

  constructor({
    appId,
    actorType,
    tabId,
  }: {
    appId: string;
    actorType: string;
    tabId: string;
  }) {
    this.appId = appId;
    this.session_id = Utils.generateId();
    this.actorType = actorType;
    this.tabId = tabId;
  }

  getActorType(): string {
    return this.actorType;
  }

  getAppId(): string {
    return this.appId;
  }

  getTabId(): string {
    return this.tabId;
  }

  getChatListeners(): Set<(data: TActrMessageResponse) => void> {
    return this.chatListeners;
  }

  getVoiceListeners(): Set<(data: TActrMessageResponse) => void> {
    return this.voiceListeners;
  }

  acquire() {
    this.refCount++;
    console.log(
      `[ActrConnection:${this.appId}] Acquired, count: ${this.refCount}`,
    );

    if (this.disconnectTimer) {
      console.log(
        `[ActrConnection:${this.appId}] Pending disconnect cancelled.`,
      );
      clearTimeout(this.disconnectTimer);
      this.disconnectTimer = null;
    }
  }

  async release() {
    this.refCount--;
    console.log(
      `[ActrConnection:${this.appId}] Released, count: ${this.refCount}`,
    );

    if (this.refCount <= 0) {
      this.refCount = 0;

      if (this.disconnectTimer) {
        clearTimeout(this.disconnectTimer);
      }

      console.log(
        `[ActrConnection:${this.appId}] Last instance released, scheduling disconnect in 2s...`,
      );
      this.disconnectTimer = setTimeout(async () => {
        this.disconnectTimer = null;
        if (this.refCount <= 0) {
          await this.disconnect();
        }
      }, 2000);
    }
  }

  subscribeChatMessage(callback: (data: TActrMessageResponse) => void) {
    this.chatListeners.add(callback);
    return () => this.unsubscribeChatMessage(callback);
  }

  unsubscribeChatMessage(callback: (data: TActrMessageResponse) => void) {
    this.chatListeners.delete(callback);
  }

  subscribeVoiceMessage(callback: (data: TActrMessageResponse) => void) {
    this.voiceListeners.add(callback);
    return () => this.unsubscribeVoiceMessage(callback);
  }

  unsubscribeVoiceMessage(callback: (data: TActrMessageResponse) => void) {
    this.voiceListeners.delete(callback);
  }

  async initialize(configPath: string): Promise<void> {
    try {
      console.log(`[ActrConnection:${this.appId}] Initializing...`);

      await ActrNativeBridge.initializeService({
        path: configPath,
        actorType: this.actorType,
        tabId: this.tabId,
      });
      this.isInitialized = true;
      this.reason = '';
      console.log(`[ActrConnection:${this.appId}] ActrService initialized.`);
    } catch (error: any) {
      this.reason = error.message || error;

      console.error(`[ActrConnection:${this.appId}] Initialize failed:`, error.message || error);
      this.isInitialized = false;
      throw error;
    }
  }

  async sendMessage(content: TActrMessageRequest): Promise<void> {
    const router = ActrStreamRouter.getInstance();
    let streamIdMap: TActrStreamMap | null = null;

    try {
      console.log(`[ActrConnection:${this.appId}] Start send message`);

      streamIdMap = router.bindStream(content.messageId, this);

      if (!this.isInitialized) {
        // When the sending stage fails, the error is converted to a local final text message, which is used to re-distribut the existing message.
        const router = ActrStreamRouter.getInstance();
        router.emitLocalTextMessage(streamIdMap.textResponseStreamId, {
          message: errorNote('Connection not initialized'),
        });
        console.error(`[ActrConnection:${this.appId}] Not initialized.`);
        return;
      }

      const req = {
        messageId: content.messageId,
        sessionId: content.sessionId,
        text: content.text,
        location: content.location,
        attachmentIds: content.attachmentIds,
        streamId: streamIdMap.requestStreamId,
        textResponseStreamId: streamIdMap.textResponseStreamId,
        voiceResponseStreamId: streamIdMap.voiceResponseStreamId,
        actorType: this.actorType,
        tabId: this.tabId,
      };

      console.log(
        `[ActrConnection:${this.appId}] Send message ${JSON.stringify(req)}`,
      );

      return await ActrNativeBridge.sendTextMessage(req);
    } catch (error: any) {
      if (streamIdMap) {
        const errorMessage = error?.message || String(error);

        // When the sending stage fails, the error is converted to a local final text message, which is used to re-distribut the existing message.
        router.emitLocalTextMessage(streamIdMap.textResponseStreamId, {
          message: errorNote(errorMessage),
        });
      }

      console.error(
        `[ActrConnection:${this.appId}] Send message failed:`,
        error.message || error,
      );
      // throw error;
    }
  }

  async startVoice(content: TActrVoiceRequest): Promise<string> {
    if (!this.isInitialized) {
      throw new Error(`[ActrConnection:${this.appId}] Not initialized.`);
    }

    try {
      const router = ActrStreamRouter.getInstance();
      const streamIdMap = router.bindStream(content.messageId, this);
      this.currentVoiceRequestStreamId = streamIdMap.requestStreamId;

      const req = {
        messageId: content.messageId,
        sessionId: content.sessionId,
        location: content.location,
        attachmentIds: content.attachmentIds,
        streamId: streamIdMap.requestStreamId,
        textResponseStreamId: streamIdMap.textResponseStreamId,
        voiceResponseStreamId: streamIdMap.voiceResponseStreamId,
        actorType: this.actorType,
        tabId: this.tabId,
      };

      console.log(
        `[ActrConnection:${this.appId}] Start voice ${JSON.stringify(req)}`,
      );
      const res = await ActrNativeBridge.startVoice(req);
      console.log(res, 'start');

      return res.streamId;
    } catch (e: any) {
      this.currentVoiceRequestStreamId = '';
      this.currentVoiceChunkCount = 0;

      console.error(
        `[ActrConnection:${this.appId}] Start voice failed:`,
        e.message,
      );
      throw e;
    }
  }

  async sendVoiceChunk(data: Uint8Array): Promise<string> {
    if (!this.isInitialized) {
      throw new Error(`[ActrConnection:${this.appId}] Not initialized.`);
    }

    try {
      this.currentVoiceChunkCount++;

      console.log('Send audio slices: ', this.currentVoiceChunkCount);

      const base64Data = Buffer.from(data).toString('base64');
      const res = await ActrNativeBridge.sendVoiceChunk(
        {
          streamId: this.currentVoiceRequestStreamId,
          chunkId: this.currentVoiceChunkCount.toString(),
          actorType: this.actorType,
          tabId: this.tabId,
        },
        base64Data,
      );
      console.log('We got an audio slice upload response.');

      return res;
    } catch (e: any) {
      console.error(
        `[ActrConnection:${this.appId}] Send voice chunk failed:`,
        e.message,
      );
      throw e;
    }
  }

  async endVoice(): Promise<string> {
    if (!this.isInitialized) {
      throw new Error(`[ActrConnection:${this.appId}] Not initialized.`);
    }

    try {
      console.log(
        `[ActrConnection:${this.appId}] End voice`,
        this.currentVoiceRequestStreamId,
      );
      const res = await ActrNativeBridge.sendVoiceChunk(
        {
          streamId: this.currentVoiceRequestStreamId,
          chunkId: (this.currentVoiceChunkCount + 1).toString(),
          actorType: this.actorType,
          tabId: this.tabId,
          metadata: [
            {
              value: ACTR_BRIDGE_CONTRACT.streamMetadata.terminalValue,
              key: ACTR_BRIDGE_CONTRACT.streamMetadata.eosKey,
            },
          ],
        },
        '',
      );
      console.log(res, 'end');

      return res;
    } catch (e: any) {
      console.error(
        `[ActrConnection:${this.appId}] End voice failed:`,
        e.message,
      );
      throw e;
    } finally {
      this.currentVoiceRequestStreamId = '';
      this.currentVoiceChunkCount = 0;
    }
  }

  // Attachment upload must wait native Back attachmentResult.completedAgain. attachmentId Bringing Text/Voice message.
  async uploadAttachment(data: TAttachmentUploadRequest): Promise<string> {
    if (!this.isInitialized) {
      throw new Error(`[ActrConnection:${this.appId}] Not initialized.`);
    }

    const router = ActrStreamRouter.getInstance();
    const chunkStreamId = Utils.generateId();
    const resultStreamId = Utils.generateId();
    const normalizedFilePath = data.filePath.replace('file://', '');
    let hasPreparedResultStream = false;

    try {
      const fileStat = await RNFS.stat(normalizedFilePath);
      const checksum = await RNFS.hash(
        normalizedFilePath,
        ATTACHMENT_CHECKSUM_ALGORITHM,
      );
      const sizeBytes = Number(fileStat.size);

      console.log(
        `[ActrConnection:${this.appId}] Start upload attachment ${data.fileName}, size=${sizeBytes}`,
      );

      const prepareResult = await ActrNativeBridge.prepareAttachmentUpload({
        tabId: this.tabId,
        id: data.attachmentId,
        filename: data.fileName,
        type: AttachmentType.DOCUMENT,
        streamId: chunkStreamId,
        checksum,
        checksumAlgorithm: ATTACHMENT_CHECKSUM_ALGORITHM,
        sizeBytes,
        resultStreamId,
      });

      // prepare Success means only native Registered Upload/Result flow, F 0 The uploading process of the annex to the note was not successful.
      if (
        prepareResult.statusCode !==
        ACTR_BRIDGE_CONTRACT.attachment.statusCode.ok
      ) {
        throw new Error(
          prepareResult.errorMessage ||
            `Prepare attachment upload failed: ${prepareResult.statusCode}`,
        );
      }
      console.log(
        `[ActrConnection:${this.appId}] Attachment upload prepared ${data.fileName}, ` +
          `attachmentId=${prepareResult.id}, chunkStreamId=${chunkStreamId}, resultStreamId=${resultStreamId}`,
      );
      hasPreparedResultStream = true;
      const attachmentResultPromise = router.waitForAttachmentResult(
        resultStreamId,
        this,
      );

      // Total fractions for log alignment native Packing up. chunk.
      const totalChunks = Math.ceil(sizeBytes / ATTACHMENT_CHUNK_SIZE);
      let chunkIndex = 0;
      for (let position = 0; position < sizeBytes; position += ATTACHMENT_CHUNK_SIZE) {
        // Annex I chunkId By 1 Starts in an incremental order,native Side presses chunkId Reverts the upload order.
        chunkIndex += 1;
        const chunkLength = Math.min(ATTACHMENT_CHUNK_SIZE, sizeBytes - position);
        // Last Attachment Segment Load eos, Notification native/The service end of the data stream of the attachment is over.
        const chunkMetadata =
          chunkIndex === totalChunks
            ? [
                {
                  key: ACTR_BRIDGE_CONTRACT.streamMetadata.eosKey,
                  value: ACTR_BRIDGE_CONTRACT.streamMetadata.terminalValue,
                },
              ]
            : [];
        console.log(
          `[ActrConnection:${this.appId}] Send attachment chunk ${data.fileName} ` +
            `${chunkIndex}/${totalChunks}, offset=${position}, bytes=${chunkLength}, ` +
            `metadata=${JSON.stringify(chunkMetadata)}`,
        );
        const chunkBase64 = await RNFS.read(
          normalizedFilePath,
          chunkLength,
          position,
          'base64',
        );
        const chunkResult = await ActrNativeBridge.sendAttachmentChunk(
          {
            tabId: this.tabId,
            sessionId: data.sessionId,
            attachmentId: data.attachmentId,
            chunkId: String(chunkIndex),
            streamId: chunkStreamId,
            metadata: chunkMetadata,
          },
          chunkBase64,
        );
        console.log(
          `[ActrConnection:${this.appId}] Attachment chunk sent ${data.fileName} ` +
            `${chunkIndex}/${totalChunks}, status=${chunkResult.status}, ` +
            `metadata=${JSON.stringify(chunkMetadata)}`,
        );
        // chunk The response is... native Synchronizes the sending of single pieces, not 0 , and then you will terminate the uploading of the entire attachment.
        if (chunkResult.status !== ACTR_BRIDGE_CONTRACT.streamStatus.ok) {
          throw new Error(
            chunkResult.message || `Send attachment chunk failed: ${chunkResult.status}`,
          );
        }
      }

      const attachmentResult = await attachmentResultPromise;
      // Only received. attachmentResult.completed and statusCode=0, annex ID It's a safe way to pass on text./Voice message.
      if (
        attachmentResult.status !== AttachmentUploadStatus.COMPLETED ||
        attachmentResult.statusCode !==
          ACTR_BRIDGE_CONTRACT.attachment.statusCode.ok
      ) {
        throw new Error(
          attachmentResult.errorMessage ||
            `Attachment upload failed: ${attachmentResult.statusCode}`,
        );
      }

      return attachmentResult.id;
    } catch (e: any) {
      // Just... prepare After success native I'm the one who really holds it. result streamThe failure of clean-up also targets only such registered streams.
      if (hasPreparedResultStream) {
        router.cancelAttachmentResultWait(resultStreamId, this);
      }
      console.error(
        `[ActrConnection:${this.appId}] Upload attachment failed:`,
        e.message || e,
      );
      throw e;
    }
  }

  async disconnect() {
    if (this.refCount > 0) {
      console.log(
        `[ActrConnection:${this.appId}] Disconnect aborted: new instance found.`,
      );
      return;
    }

    console.log(`[ActrConnection:${this.appId}] Disconnecting...`);
    try {
      await ActrNativeBridge.shutdownService({
        actorType: this.actorType,
        tabId: this.tabId,
      });

      if (this.refCount > 0) {
        console.log(
          `[ActrConnection:${this.appId}] Disconnect rollbacked because a new instance acquired during shutdown.`,
        );
        return;
      }

      ActrStreamRouter.getInstance().clearConnectionStreams(this);

      this.isInitialized = false;
      this.chatListeners.clear();
      this.voiceListeners.clear();
      console.log(`[ActrConnection:${this.appId}] Disconnected.`);
    } catch (error: any) {
      console.error(
        `[ActrConnection:${this.appId}] Cleanup failed:`,
        error.message,
      );
    }
  }

  isConnected(): boolean {
    return this.isInitialized;
  }
}
