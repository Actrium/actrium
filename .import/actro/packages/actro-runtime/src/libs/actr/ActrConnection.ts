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

// 附件上传按 48KB 固定分片，RN 与 native 依赖同一切片大小约定保证 chunk 顺序与边界一致。
const ATTACHMENT_CHUNK_SIZE = ACTR_BRIDGE_CONTRACT.attachment.chunkSizeBytes;
const ATTACHMENT_CHECKSUM_ALGORITHM =
  ACTR_BRIDGE_CONTRACT.attachment.checksumAlgorithms.md5;

/**
 * Actr 连接管理类
 * 每个实例对应一个特定 App 的连接。
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
        // 发送阶段失败时，将错误转换为一条本地终态文本消息，复用既有消息分发链路。
        const router = ActrStreamRouter.getInstance();
        router.emitLocalTextMessage(streamIdMap.textResponseStreamId, {
          message: errorNote('连接未初始化'),
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

        // 发送阶段失败时，将错误转换为一条本地终态文本消息，复用既有消息分发链路。
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

      console.log('发送音频切片: ', this.currentVoiceChunkCount);

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
      console.log('收到音频切片上传响应');

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

  // 附件上传必须先等待 native 返回 attachmentResult.completed，再把 attachmentId 带进文本/语音消息。
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

      // prepare 成功只表示 native 已注册上传/结果流，非 0 说明附件上传流程连入口都没建立成功。
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

      // 总分片数用于日志对齐 native 收包进度，便于定位卡在哪个 chunk。
      const totalChunks = Math.ceil(sizeBytes / ATTACHMENT_CHUNK_SIZE);
      let chunkIndex = 0;
      for (let position = 0; position < sizeBytes; position += ATTACHMENT_CHUNK_SIZE) {
        // 同一附件的 chunkId 由 1 开始顺序递增，native 侧会按 chunkId 还原上传顺序。
        chunkIndex += 1;
        const chunkLength = Math.min(ATTACHMENT_CHUNK_SIZE, sizeBytes - position);
        // 最后一个附件分片显式携带 eos，通知 native/服务端该附件的数据流已经结束。
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
        // chunk 响应是 native 对单片发送的同步确认，非 0 时直接终止整条附件上传链路。
        if (chunkResult.status !== ACTR_BRIDGE_CONTRACT.streamStatus.ok) {
          throw new Error(
            chunkResult.message || `Send attachment chunk failed: ${chunkResult.status}`,
          );
        }
      }

      const attachmentResult = await attachmentResultPromise;
      // 只有收到 attachmentResult.completed 且 statusCode=0，附件 ID 才能安全透传给文本/语音消息。
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
      // 只有 prepare 成功后 native 才真正持有 result stream，失败清理也只针对这类已注册流。
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
