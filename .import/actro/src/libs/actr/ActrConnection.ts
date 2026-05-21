import { Buffer } from 'buffer';
import {
  TActrMessageRequest,
  TActrMessageResponse,
  TActrVoiceRequest,
} from './ActrConnection.type';
import { ActrNativeBridge } from '../../native-bridge/actr/ActrNativeBridge';
import { Utils } from '../../utils/utils';
import { ActrStreamRouter, TActrStreamMap } from './ActrStreamRouter';
import { errorNote } from './LocalTestMessage';

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
              value: 'true',
              key: 'eos',
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

  async uploadAttachment(
    streamId: string,
    data: {
      fileName: string;
      fieldData: Uint8Array;
    },
  ): Promise<string> {
    if (!this.isInitialized) {
      console.error(`[ActrConnection:${this.appId}] Not initialized. Cannot upload attachment.`);
      return '';
    }
    console.log('发送附件', data.fileName, data.fieldData.length);

    try {
      const res = await ActrNativeBridge.uploadAttachment({
        streamId,
        type: 2,
        fileData: Utils.uint8ArrayToBase64(data.fieldData),
        actorType: this.actorType,
        tabId: this.tabId,
        filename: data.fileName
      });

      if (res.statusCode !== 0) {
        console.error(
          `[ActrConnection:${this.appId}] Upload attachment failed: ${res.statusCode}, ${JSON.stringify(res)}`,
        );
      }

      return res.id;
    } catch (e: any) {
      console.error(
        `[ActrConnection:${this.appId}] Upload attachment failed:`,
        e.message || e,
      );
      return '';
    }
  }

  async uploadAttachmentSlice(
    streamId: string,
    data: {
      fieldData: Uint8Array;
    },
  ): Promise<string> {
    if (!this.isInitialized) {
      throw new Error(`[ActrConnection:${this.appId}] Not initialized.`);
    }
    console.log('发送附件', data.fieldData.length);

    return '';
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
